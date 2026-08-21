//! Exclusive file creation for the built-in `file touch` command.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use chrono::{DateTime, Utc};

use crate::config::{Config, FileConfig};
use crate::error::RagtagError;
use crate::parser;

/// Synthetic source name used while validating individual CLI tags.
const TAG_VALIDATION_SOURCE: &str = "<file-touch-tag>";

/// Parsed editor executable and its configured initial arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedEditor {
    program: OsString,
    args: Vec<OsString>,
}

/// Abstraction over editor process execution for deterministic tests.
trait EditorLauncher {
    /// Launches an editor, appending `target` as its final argument.
    fn launch(&self, editor: &PreparedEditor, target: &Path) -> io::Result<ExitStatus>;
}

/// Production editor launcher using direct process execution.
struct SystemEditorLauncher;

impl EditorLauncher for SystemEditorLauncher {
    fn launch(&self, editor: &PreparedEditor, target: &Path) -> io::Result<ExitStatus> {
        Command::new(&editor.program)
            .args(&editor.args)
            .arg(target)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
    }
}

/// Runs `file touch` using the current UTC time and the system editor.
pub fn run_touch(
    matches: &clap::ArgMatches,
    config: &Config,
    root_dir: &Path,
    startup_cwd: &Path,
    stdout: &mut dyn Write,
) -> Result<(), RagtagError> {
    let tags = matches
        .get_many::<String>("tag")
        .map(|values| values.map(String::as_str))
        .into_iter()
        .flatten();
    let explicit_path = matches.get_one::<String>("path").map(Path::new);
    run_touch_at(
        tags,
        explicit_path,
        matches.get_flag("edit"),
        &config.files,
        root_dir,
        startup_cwd,
        Utc::now(),
        &SystemEditorLauncher,
        || std::env::var_os("EDITOR"),
        stdout,
    )
}

/// Executes file creation with injected time, editor, environment lookup, and output.
#[allow(clippy::too_many_arguments)]
fn run_touch_at<'a>(
    tag_values: impl IntoIterator<Item = &'a str>,
    explicit_path: Option<&Path>,
    edit: bool,
    file_config: &FileConfig,
    root_dir: &Path,
    startup_cwd: &Path,
    now: DateTime<Utc>,
    launcher: &dyn EditorLauncher,
    editor_env: impl FnOnce() -> Option<OsString>,
    stdout: &mut dyn Write,
) -> Result<(), RagtagError> {
    let tags = normalize_tags(tag_values)?;
    let target = resolve_target(explicit_path, file_config, root_dir, startup_cwd, now)?;
    let output_path = std::path::absolute(&target).map_err(RagtagError::Io)?;
    let editor = if edit {
        Some(prepare_editor(editor_env())?)
    } else {
        None
    };
    let header = build_header(&tags);
    create_new_file(&target, &header)?;

    if let Some(editor) = editor {
        let status =
            launcher
                .launch(&editor, &target)
                .map_err(|source| RagtagError::EditorLaunch {
                    path: target.clone(),
                    source,
                })?;
        if !status.success() {
            return Err(RagtagError::EditorExit {
                path: target,
                status,
            });
        }
    }
    writeln!(stdout, "{}", output_path.display()).map_err(RagtagError::Io)
}

/// Normalizes, parser-validates, and stably deduplicates supplied tags.
fn normalize_tags<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<String>, RagtagError> {
    let mut seen = HashSet::new();
    let mut normalized_tags = Vec::new();
    for original in values {
        let trimmed = original.trim();
        if trimmed.is_empty() {
            return Err(RagtagError::InvalidTag {
                input: original.to_string(),
                reason: "tag must not be empty".to_string(),
            });
        }
        let normalized = if trimmed.starts_with('@') {
            trimmed.to_string()
        } else {
            format!("@{trimmed}")
        };
        let parsed = parser::scan_file(&normalized, Path::new(TAG_VALIDATION_SOURCE));
        let valid = parsed.len() == 1 && parsed[0].raw_span == (0..normalized.len());
        if !valid {
            return Err(RagtagError::InvalidTag {
                input: original.to_string(),
                reason: "expected exactly one complete tag expression".to_string(),
            });
        }
        if seen.insert(normalized.clone()) {
            normalized_tags.push(normalized);
        }
    }
    Ok(normalized_tags)
}

/// Resolves an explicit or UTC-generated target without canonicalization.
fn resolve_target(
    explicit_path: Option<&Path>,
    file_config: &FileConfig,
    root_dir: &Path,
    startup_cwd: &Path,
    now: DateTime<Utc>,
) -> Result<PathBuf, RagtagError> {
    if let Some(path) = explicit_path {
        validate_explicit_path(path)?;
        return Ok(if path.is_absolute() {
            path.to_path_buf()
        } else {
            startup_cwd.join(path)
        });
    }

    let filename = now.format(&file_config.filename_format).to_string();
    validate_generated_filename(&filename)?;
    let directory = if file_config.default_directory.is_absolute() {
        file_config.default_directory.clone()
    } else {
        root_dir.join(&file_config.default_directory)
    };
    Ok(directory.join(filename))
}

/// Rejects explicit targets without a normal final filename component.
fn validate_explicit_path(path: &Path) -> Result<(), RagtagError> {
    let source = path.as_os_str().to_string_lossy();
    let has_trailing_separator =
        source.ends_with(std::path::MAIN_SEPARATOR) || cfg!(windows) && source.ends_with('/');
    #[cfg(windows)]
    let final_source_component = source.rsplit(['/', '\\']).next().unwrap_or_default();
    #[cfg(not(windows))]
    let final_source_component = source
        .rsplit(std::path::MAIN_SEPARATOR)
        .next()
        .unwrap_or_default();
    if path.as_os_str().is_empty()
        || has_trailing_separator
        || matches!(final_source_component, "." | "..")
        || !matches!(path.components().next_back(), Some(Component::Normal(_)))
    {
        return Err(RagtagError::InvalidFileTarget {
            path: path.to_path_buf(),
            reason: "path must end with a usable filename".to_string(),
        });
    }
    Ok(())
}

/// Ensures rendered strftime output is exactly one normal path component.
fn validate_generated_filename(filename: &str) -> Result<(), RagtagError> {
    let path = Path::new(filename);
    let mut components = path.components();
    let valid = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && filename != "."
        && filename != "..";
    if !valid {
        return Err(RagtagError::InvalidFileTarget {
            path: path.to_path_buf(),
            reason: "generated filename must be exactly one normal filename component".to_string(),
        });
    }
    Ok(())
}

/// Serializes normalized tags at byte zero with one LF after each tag.
fn build_header(tags: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for tag in tags {
        bytes.extend_from_slice(tag.as_bytes());
        bytes.push(b'\n');
    }
    bytes
}

/// Parses EDITOR as shell words without invoking a shell.
fn prepare_editor(value: Option<OsString>) -> Result<PreparedEditor, RagtagError> {
    let value = value.ok_or_else(|| {
        RagtagError::InvalidEditor("EDITOR is unset; set it or omit --edit".to_string())
    })?;
    let text = value.into_string().map_err(|_| {
        RagtagError::InvalidEditor("EDITOR must contain valid Unicode shell words".to_string())
    })?;
    if text.trim().is_empty() {
        return Err(RagtagError::InvalidEditor(
            "EDITOR must not be blank".to_string(),
        ));
    }
    let mut tokens = shlex::split(&text).ok_or_else(|| {
        RagtagError::InvalidEditor("EDITOR contains malformed shell quoting".to_string())
    })?;
    if tokens.first().is_none_or(String::is_empty) {
        return Err(RagtagError::InvalidEditor(
            "EDITOR must name an executable".to_string(),
        ));
    }
    let program = OsString::from(tokens.remove(0));
    Ok(PreparedEditor {
        program,
        args: tokens.into_iter().map(OsString::from).collect(),
    })
}

/// Recursively creates parents, then exclusively creates and writes a target.
fn create_new_file(target: &Path, contents: &[u8]) -> Result<(), RagtagError> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| RagtagError::FileParentCreate {
        parent: parent.to_path_buf(),
        target: target.to_path_buf(),
        source,
    })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                RagtagError::FileTargetExists(target.to_path_buf())
            } else {
                RagtagError::FileCreate {
                    path: target.to_path_buf(),
                    source,
                }
            }
        })?;
    file.write_all(contents)
        .and_then(|()| file.flush())
        .map_err(|source| RagtagError::FileCreateWrite {
            path: target.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};
    use std::ffi::OsStr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    /// Launcher that records invocations and returns a chosen process status.
    struct FakeLauncher {
        calls: AtomicUsize,
        status: ExitStatus,
    }

    impl EditorLauncher for FakeLauncher {
        fn launch(&self, _editor: &PreparedEditor, _target: &Path) -> io::Result<ExitStatus> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.status)
        }
    }

    #[cfg(unix)]
    fn exit_status(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn test_normalize_tags_validates_preserves_order_and_deduplicates() {
        let tags = normalize_tags([
            " todo ",
            "@task(owner=\"A B\", priority=1)",
            "@todo",
            "@task(owner=\"C\", priority=1)",
        ])
        .unwrap();
        assert_eq!(
            tags,
            [
                "@todo",
                "@task(owner=\"A B\", priority=1)",
                "@task(owner=\"C\", priority=1)"
            ]
        );
        assert_eq!(
            build_header(&tags),
            b"@todo\n@task(owner=\"A B\", priority=1)\n@task(owner=\"C\", priority=1)\n"
        );
        assert!(build_header(&[]).is_empty());
    }

    #[test]
    fn test_normalize_tags_rejects_incomplete_or_multiple_input() {
        for value in ["", " ", "@123", "@one trailing", "@one @two", "@one("] {
            assert!(normalize_tags([value]).is_err(), "{value:?} should fail");
        }
    }

    #[test]
    fn test_resolve_target_uses_cwd_root_and_fixed_utc() {
        let now = Utc.with_ymd_and_hms(2026, 8, 21, 12, 33, 52).unwrap();
        let config = FileConfig::default();
        assert_eq!(
            resolve_target(None, &config, Path::new("/root"), Path::new("/cwd"), now).unwrap(),
            PathBuf::from("/root/./2026-08-21_12-33-52.md")
        );
        assert_eq!(
            resolve_target(
                Some(Path::new("../note.md")),
                &config,
                Path::new("/root"),
                Path::new("/cwd/sub"),
                now
            )
            .unwrap(),
            PathBuf::from("/cwd/sub/../note.md")
        );
        assert_eq!(
            resolve_target(
                Some(Path::new("/outside/note.md")),
                &config,
                Path::new("/root"),
                Path::new("/cwd"),
                now
            )
            .unwrap(),
            PathBuf::from("/outside/note.md")
        );
    }

    #[test]
    fn test_resolve_target_supports_fractional_format_and_rejects_pathlike_output() {
        let now = Utc
            .with_ymd_and_hms(2026, 8, 21, 12, 33, 52)
            .unwrap()
            .with_nanosecond(123_000_000)
            .unwrap();
        let mut config = FileConfig {
            default_directory: PathBuf::from("/notes"),
            filename_format: "%Y%m%d-%3f.txt".to_string(),
        };
        assert_eq!(
            resolve_target(None, &config, Path::new("/root"), Path::new("/cwd"), now).unwrap(),
            PathBuf::from("/notes/20260821-123.txt")
        );
        for invalid in ["", ".", "..", "dir/name"] {
            config.filename_format = invalid.to_string();
            assert!(
                resolve_target(None, &config, Path::new("/root"), Path::new("/cwd"), now).is_err()
            );
        }
        assert!(resolve_target(
            Some(Path::new("")),
            &FileConfig::default(),
            Path::new("/root"),
            Path::new("/cwd"),
            now
        )
        .is_err());
        for invalid in ["new-parent/.", "new-parent/..", "new-parent/"] {
            assert!(resolve_target(
                Some(Path::new(invalid)),
                &FileConfig::default(),
                Path::new("/root"),
                Path::new("/cwd"),
                now
            )
            .is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_target_allows_backslashes_in_terminal_filename() {
        let now = Utc.with_ymd_and_hms(2026, 8, 21, 12, 33, 52).unwrap();
        for valid in [r"note\.", r"note\.."] {
            assert_eq!(
                resolve_target(
                    Some(Path::new(valid)),
                    &FileConfig::default(),
                    Path::new("/root"),
                    Path::new("/cwd"),
                    now
                )
                .unwrap(),
                Path::new("/cwd").join(valid)
            );
        }
    }

    #[test]
    fn test_create_new_file_creates_parents_and_rejects_existing_entries() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a/b/note.md");
        create_new_file(&target, b"@one\n").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"@one\n");
        assert!(matches!(
            create_new_file(&target, b"replacement"),
            Err(RagtagError::FileTargetExists(_))
        ));
        assert_eq!(fs::read(&target).unwrap(), b"@one\n");

        let directory = dir.path().join("existing-directory");
        fs::create_dir(&directory).unwrap();
        assert!(matches!(
            create_new_file(&directory, b""),
            Err(RagtagError::FileTargetExists(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn test_create_new_file_rejects_symlinks_including_dangling() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original");
        fs::write(&original, b"original").unwrap();
        let link = dir.path().join("link");
        symlink(&original, &link).unwrap();
        assert!(matches!(
            create_new_file(&link, b"new"),
            Err(RagtagError::FileTargetExists(_))
        ));
        assert_eq!(fs::read(&original).unwrap(), b"original");

        let dangling = dir.path().join("dangling");
        symlink(dir.path().join("missing"), &dangling).unwrap();
        assert!(matches!(
            create_new_file(&dangling, b"new"),
            Err(RagtagError::FileTargetExists(_))
        ));
    }

    #[test]
    fn test_create_new_file_has_exactly_one_concurrent_winner() {
        let dir = tempfile::tempdir().unwrap();
        let target = Arc::new(dir.path().join("race.md"));
        let barrier = Arc::new(Barrier::new(8));
        let handles = (0..8)
            .map(|_| {
                let target = Arc::clone(&target);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    create_new_file(&target, b"winner")
                })
            })
            .collect::<Vec<_>>();
        let successes = handles
            .into_iter()
            .map(|handle| usize::from(handle.join().unwrap().is_ok()))
            .sum::<usize>();
        assert_eq!(successes, 1);
        assert_eq!(fs::read(&*target).unwrap(), b"winner");
    }

    #[test]
    fn test_prepare_editor_parses_program_and_quoted_arguments() {
        let editor = prepare_editor(Some(OsString::from("editor --flag 'two words'"))).unwrap();
        assert_eq!(editor.program, OsStr::new("editor"));
        assert_eq!(
            editor.args,
            [OsString::from("--flag"), OsString::from("two words")]
        );
        assert!(prepare_editor(None).is_err());
        assert!(prepare_editor(Some(OsString::from("  "))).is_err());
        assert!(prepare_editor(Some(OsString::from("'unterminated"))).is_err());
        for empty_program in ["''", "'' --wait"] {
            assert!(matches!(
                prepare_editor(Some(OsString::from(empty_program))),
                Err(RagtagError::InvalidEditor(_))
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_touch_edit_ordering_and_file_retention() {
        let dir = tempfile::tempdir().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 8, 21, 12, 33, 52).unwrap();
        let success = FakeLauncher {
            calls: AtomicUsize::new(0),
            status: exit_status(0),
        };
        let no_edit = dir.path().join("no-edit.md");
        let mut output = Vec::new();
        run_touch_at(
            ["todo"],
            Some(Path::new("no-edit.md")),
            false,
            &FileConfig::default(),
            dir.path(),
            dir.path(),
            now,
            &success,
            || panic!("EDITOR must not be consulted without --edit"),
            &mut output,
        )
        .unwrap();
        assert_eq!(success.calls.load(Ordering::SeqCst), 0);
        assert_eq!(fs::read(no_edit).unwrap(), b"@todo\n");
        assert_eq!(
            output,
            format!("{}\n", dir.path().join("no-edit.md").display()).as_bytes()
        );

        let prevalidation = dir.path().join("prevalidation.md");
        output.clear();
        let result = run_touch_at(
            [],
            Some(Path::new("prevalidation.md")),
            true,
            &FileConfig::default(),
            dir.path(),
            dir.path(),
            now,
            &success,
            || None,
            &mut output,
        );
        assert!(matches!(result, Err(RagtagError::InvalidEditor(_))));
        assert!(!prevalidation.exists());
        assert!(output.is_empty());

        let empty_program_parent = dir.path().join("empty-program-parent");
        output.clear();
        let result = run_touch_at(
            [],
            Some(Path::new("empty-program-parent/note.md")),
            true,
            &FileConfig::default(),
            dir.path(),
            dir.path(),
            now,
            &success,
            || Some(OsString::from("'' --wait")),
            &mut output,
        );
        assert!(matches!(result, Err(RagtagError::InvalidEditor(_))));
        assert!(!empty_program_parent.exists());
        assert!(output.is_empty());

        let failure = FakeLauncher {
            calls: AtomicUsize::new(0),
            status: exit_status(7),
        };
        let retained = dir.path().join("retained.md");
        output.clear();
        let result = run_touch_at(
            [],
            Some(Path::new("retained.md")),
            true,
            &FileConfig::default(),
            dir.path(),
            dir.path(),
            now,
            &failure,
            || Some(OsString::from("editor")),
            &mut output,
        );
        assert!(matches!(result, Err(RagtagError::EditorExit { .. })));
        assert!(retained.exists());
        assert!(output.is_empty());

        let edited = dir.path().join("edited.md");
        output.clear();
        run_touch_at(
            [],
            Some(Path::new("edited.md")),
            true,
            &FileConfig::default(),
            dir.path(),
            dir.path(),
            now,
            &success,
            || Some(OsString::from("editor")),
            &mut output,
        )
        .unwrap();
        assert!(edited.exists());
        assert_eq!(output, format!("{}\n", edited.display()).as_bytes());
    }
}
