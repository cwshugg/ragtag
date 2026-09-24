//! Diagram stdout and reviewed atomic file sinks.

use std::io::Write;
use std::path::Path;

use crate::error::RagtagError;

pub(crate) fn write_stdout(bytes: &[u8], output: &mut dyn Write) -> Result<(), RagtagError> {
    output.write_all(bytes).map_err(RagtagError::StdoutWrite)?;
    output.flush().map_err(RagtagError::StdoutFlush)
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct CommitFailure {
    committed: bool,
    source: std::io::Error,
}

#[cfg(target_os = "linux")]
fn commit_sequence(
    mut precommit: impl FnMut() -> std::io::Result<()>,
    mut close: impl FnMut(),
    mut rename: impl FnMut() -> std::io::Result<()>,
    mut sync_directory: impl FnMut() -> std::io::Result<()>,
    mut cleanup: impl FnMut(),
) -> Result<(), CommitFailure> {
    if let Err(source) = precommit() {
        close();
        cleanup();
        return Err(CommitFailure {
            committed: false,
            source,
        });
    }
    close();
    if let Err(source) = rename() {
        cleanup();
        return Err(CommitFailure {
            committed: false,
            source,
        });
    }
    sync_directory().map_err(|source| CommitFailure {
        committed: true,
        source,
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), RagtagError> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| RagtagError::InvalidFileTarget {
            path: path.to_path_buf(),
            reason: "output must name a file".to_string(),
        })?;
    let directory = rustix::fs::openat2(
        rustix::fs::CWD,
        parent,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
        rustix::fs::ResolveFlags::NO_SYMLINKS,
    )
    .map_err(|source| RagtagError::FileWrite {
        path: path.to_path_buf(),
        source: source.into(),
    })?;
    validate_destination(&directory, file_name, path)?;

    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|error| RagtagError::FileWrite {
        path: path.to_path_buf(),
        source: std::io::Error::other(error.to_string()),
    })?;
    let temporary_name = format!(
        ".ragtag-diagram-{}",
        random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let temporary_fd = rustix::fs::openat(
        &directory,
        temporary_name.as_str(),
        rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .map_err(|source| RagtagError::FileWrite {
        path: path.to_path_buf(),
        source: source.into(),
    })?;
    let temporary = std::cell::RefCell::new(Some(std::fs::File::from(temporary_fd)));
    let result = commit_sequence(
        || {
            let mut slot = temporary.borrow_mut();
            let file = slot.as_mut().expect("temporary file remains open");
            file.write_all(bytes)
                .and_then(|()| file.flush())
                .and_then(|()| {
                    rustix::fs::fchmod(&*file, rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR)
                        .map_err(Into::into)
                })
                .and_then(|()| file.sync_all())
                .and_then(|()| validate_destination_io(&directory, file_name))
        },
        || {
            temporary.borrow_mut().take();
        },
        || {
            rustix::fs::renameat(&directory, temporary_name.as_str(), &directory, file_name)
                .map_err(Into::into)
        },
        || rustix::fs::fsync(&directory).map_err(Into::into),
        || {
            let _ = rustix::fs::unlinkat(
                &directory,
                temporary_name.as_str(),
                rustix::fs::AtFlags::empty(),
            );
        },
    );
    result.map_err(|failure| {
        if failure.committed {
            RagtagError::FileCommittedDurabilityUncertain {
                path: path.to_path_buf(),
                source: failure.source,
            }
        } else {
            RagtagError::FileWrite {
                path: path.to_path_buf(),
                source: failure.source,
            }
        }
    })
}

#[cfg(target_os = "linux")]
fn validate_destination(
    directory: &impl std::os::fd::AsFd,
    file_name: &std::ffi::OsStr,
    path: &Path,
) -> Result<(), RagtagError> {
    validate_destination_io(directory, file_name).map_err(|source| RagtagError::InvalidFileTarget {
        path: path.to_path_buf(),
        reason: source.to_string(),
    })
}

#[cfg(target_os = "linux")]
fn validate_destination_io(
    directory: &impl std::os::fd::AsFd,
    file_name: &std::ffi::OsStr,
) -> std::io::Result<()> {
    match rustix::fs::statat(directory, file_name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) if rustix::fs::FileType::from_raw_mode(stat.st_mode).is_file() => Ok(()),
        Ok(_) => Err(std::io::Error::other(
            "existing output must be a regular file, not a symlink",
        )),
        Err(rustix::io::Errno::NOENT) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn write_file(_path: &Path, _bytes: &[u8]) -> Result<(), RagtagError> {
    Err(RagtagError::SecureFileOutputUnsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PrefixFailure {
        remaining: usize,
        written: Vec<u8>,
        fail_flush: bool,
    }

    impl Write for PrefixFailure {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("injected write failure"));
            }
            let length = self.remaining.min(bytes.len());
            self.written.extend_from_slice(&bytes[..length]);
            self.remaining -= length;
            Ok(length)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.fail_flush {
                Err(std::io::Error::other("injected flush failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn stdout_may_contain_a_prefix_when_writing_fails() {
        let mut writer = PrefixFailure {
            remaining: 3,
            written: Vec::new(),
            fail_flush: false,
        };
        assert!(matches!(
            write_stdout(b"abcdef", &mut writer),
            Err(RagtagError::StdoutWrite(_))
        ));
        assert_eq!(writer.written, b"abc");
    }

    #[test]
    fn stdout_reports_flush_failure_after_complete_write() {
        let mut writer = PrefixFailure {
            remaining: usize::MAX,
            written: Vec::new(),
            fail_flush: true,
        };
        assert!(matches!(
            write_stdout(b"complete", &mut writer),
            Err(RagtagError::StdoutFlush(_))
        ));
        assert_eq!(writer.written, b"complete");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn commit_sequence_cleans_every_precommit_failure() {
        for fail_rename in [false, true] {
            let closed = std::cell::Cell::new(false);
            let cleaned = std::cell::Cell::new(false);
            let result = commit_sequence(
                || {
                    if fail_rename {
                        Ok(())
                    } else {
                        Err(std::io::Error::other("precommit"))
                    }
                },
                || closed.set(true),
                || {
                    if fail_rename {
                        Err(std::io::Error::other("rename"))
                    } else {
                        Ok(())
                    }
                },
                || Ok(()),
                || cleaned.set(true),
            );
            let failure = result.unwrap_err();
            assert!(!failure.committed);
            assert!(closed.get());
            assert!(cleaned.get());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn post_rename_sync_failure_is_distinct_and_not_cleaned() {
        let destination_changed = std::cell::Cell::new(false);
        let cleaned = std::cell::Cell::new(false);
        let failure = commit_sequence(
            || Ok(()),
            || {},
            || {
                destination_changed.set(true);
                Ok(())
            },
            || Err(std::io::Error::other("directory sync")),
            || cleaned.set(true),
        )
        .unwrap_err();
        assert!(failure.committed);
        assert!(destination_changed.get());
        assert!(!cleaned.get());
    }
}
