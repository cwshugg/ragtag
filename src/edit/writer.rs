//! Atomic in-place file editing.
//!
//! Provides safe file editing via a tempfile + rename strategy that
//! preserves file permissions and ensures atomic replacement.

use std::io::Write;
use std::ops::Range;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use crate::error::RagtagError;
use crate::parser;

/// Trait for in-place file editing, enabling testability.
pub trait FileEditor {
    /// Updates a specific attribute within a tag in a file.
    ///
    /// Finds the tag at `tag_span`, locates or inserts the attribute
    /// named `attr_name`, and sets its value to `new_value`.
    fn update_tag_attribute(
        &self,
        file_path: &Path,
        tag_span: Range<usize>,
        attr_name: &str,
        new_value: &str,
    ) -> Result<(), RagtagError>;
}

/// Atomic file editor using tempfile + rename.
pub struct AtomicFileEditor;

impl FileEditor for AtomicFileEditor {
    fn update_tag_attribute(
        &self,
        file_path: &Path,
        tag_span: Range<usize>,
        attr_name: &str,
        new_value: &str,
    ) -> Result<(), RagtagError> {
        // Open the file with O_NOFOLLOW (on Unix) to atomically reject symlinks.
        // This avoids the TOCTOU race between a symlink check and the file read.
        let content = {
            #[cfg(unix)]
            {
                use std::io::Read;
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(file_path)
                    .map_err(|e| {
                        // ELOOP indicates it's a symlink
                        if e.raw_os_error() == Some(libc::ELOOP) {
                            RagtagError::SymlinkEdit(file_path.to_path_buf())
                        } else {
                            RagtagError::FileRead {
                                path: file_path.to_path_buf(),
                                source: e,
                            }
                        }
                    })?;
                let mut buf = String::new();
                file.read_to_string(&mut buf)
                    .map_err(|e| RagtagError::FileRead {
                        path: file_path.to_path_buf(),
                        source: e,
                    })?;
                buf
            }
            #[cfg(not(unix))]
            {
                // On non-Unix platforms, fall back to the symlink_metadata check
                let meta =
                    std::fs::symlink_metadata(file_path).map_err(|e| RagtagError::FileRead {
                        path: file_path.to_path_buf(),
                        source: e,
                    })?;
                if meta.file_type().is_symlink() {
                    return Err(RagtagError::SymlinkEdit(file_path.to_path_buf()));
                }
                std::fs::read_to_string(file_path).map_err(|e| RagtagError::FileRead {
                    path: file_path.to_path_buf(),
                    source: e,
                })?
            }
        };

        // Read original permissions (using symlink_metadata to not follow symlinks)
        let original_perms = std::fs::symlink_metadata(file_path)
            .map_err(|e| RagtagError::FileRead {
                path: file_path.to_path_buf(),
                source: e,
            })?
            .permissions();

        // Validate tag span bounds before slicing
        if tag_span.start > tag_span.end
            || tag_span.end > content.len()
            || !content.is_char_boundary(tag_span.start)
            || !content.is_char_boundary(tag_span.end)
        {
            return Err(RagtagError::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: format!(
                    "tag span {}..{} is out of bounds or on invalid UTF-8 boundary (file length: {})",
                    tag_span.start, tag_span.end, content.len()
                ),
            });
        }

        // Extract tag text
        let tag_text = &content[tag_span.clone()];

        // Re-parse tag to find attribute positions
        let modified_tag_text = modify_tag_attribute(tag_text, attr_name, new_value)?;

        // Reconstruct full content
        let mut new_content = String::with_capacity(content.len());
        new_content.push_str(&content[..tag_span.start]);
        new_content.push_str(&modified_tag_text);
        new_content.push_str(&content[tag_span.end..]);

        // Write atomically
        let parent = file_path.parent().ok_or_else(|| RagtagError::FileWrite {
            path: file_path.to_path_buf(),
            source: std::io::Error::other("cannot determine parent directory"),
        })?;

        let mut tmpfile =
            tempfile::NamedTempFile::new_in(parent).map_err(|e| RagtagError::FileWrite {
                path: file_path.to_path_buf(),
                source: e,
            })?;

        tmpfile
            .write_all(new_content.as_bytes())
            .map_err(|e| RagtagError::FileWrite {
                path: file_path.to_path_buf(),
                source: e,
            })?;

        tmpfile
            .as_file()
            .sync_all()
            .map_err(|e| RagtagError::FileWrite {
                path: file_path.to_path_buf(),
                source: e,
            })?;

        // Preserve permissions
        std::fs::set_permissions(tmpfile.path(), original_perms).map_err(|e| {
            RagtagError::FileWrite {
                path: file_path.to_path_buf(),
                source: e,
            }
        })?;

        // Atomic rename
        tmpfile
            .persist(file_path)
            .map_err(|e| RagtagError::FileWrite {
                path: file_path.to_path_buf(),
                source: e.error,
            })?;

        Ok(())
    }
}

/// Modifies a tag attribute within the tag text.
///
/// If the attribute exists, replaces its value. If not, inserts it
/// before the closing `)`.
///
/// This is a pure function that operates on an in-memory string,
/// making it usable both for file editing and for `--no-edit` output.
pub fn modify_tag_attribute(
    tag_text: &str,
    attr_name: &str,
    new_value: &str,
) -> Result<String, RagtagError> {
    // Find the opening paren
    let paren_start = match tag_text.find('(') {
        Some(pos) => pos,
        None => {
            // Tag has no parens — append (attr_name=new_value)
            return Ok(format!("{tag_text}({attr_name}={new_value})"));
        }
    };

    // Re-parse the tag to find the attribute
    let tags = parser::scan_file(tag_text, Path::new(""));
    if tags.is_empty() {
        return Err(RagtagError::ParseError {
            file: "".into(),
            line: 0,
            message: "failed to re-parse tag for editing".to_string(),
        });
    }
    let tag = &tags[0];

    // Check if attribute already exists
    for attr in &tag.attributes {
        if let crate::models::AttributeKind::Named { name, .. } = &attr.kind {
            if name == attr_name {
                // Find this attribute's value position in the text and replace it
                return replace_attribute_value(tag_text, attr_name, new_value);
            }
        }
    }

    // Attribute doesn't exist — insert before closing ')'
    let close_paren = tag_text.rfind(')').unwrap_or(tag_text.len());
    let before = &tag_text[..close_paren];
    let after = &tag_text[close_paren..];

    // Check if there are existing attrs
    let inner = tag_text[paren_start + 1..close_paren].trim();
    let separator = if inner.is_empty() { "" } else { ", " };

    Ok(format!("{before}{separator}{attr_name}={new_value}{after}"))
}

/// Replaces the value of a named attribute in the raw tag text.
fn replace_attribute_value(
    tag_text: &str,
    attr_name: &str,
    new_value: &str,
) -> Result<String, RagtagError> {
    // Find `attr_name=` or `attr_name =` in the tag text
    // We need to find the attribute name followed by optional whitespace and `=`
    let mut search_pos = 0;
    let bytes = tag_text.as_bytes();

    while search_pos < tag_text.len() {
        // Find next occurrence of attr_name
        if let Some(idx) = tag_text[search_pos..].find(attr_name) {
            let abs_idx = search_pos + idx;
            let after_name = abs_idx + attr_name.len();

            // Check it's not part of a longer word
            let before_ok = abs_idx == 0
                || (!bytes[abs_idx - 1].is_ascii_alphanumeric()
                    && bytes[abs_idx - 1] != b'_'
                    && bytes[abs_idx - 1] != b'-');

            if before_ok && after_name < tag_text.len() {
                // Skip whitespace after name
                let mut eq_pos = after_name;
                while eq_pos < tag_text.len() && tag_text.as_bytes()[eq_pos].is_ascii_whitespace() {
                    eq_pos += 1;
                }

                if eq_pos < tag_text.len() && tag_text.as_bytes()[eq_pos] == b'=' {
                    // Found the attribute — now find the value span
                    let value_start = eq_pos + 1;
                    // Skip whitespace after =
                    let mut vs = value_start;
                    while vs < tag_text.len() && tag_text.as_bytes()[vs].is_ascii_whitespace() {
                        vs += 1;
                    }

                    // Determine value end
                    let value_end = if vs < tag_text.len()
                        && (tag_text.as_bytes()[vs] == b'"' || tag_text.as_bytes()[vs] == b'\'')
                    {
                        let quote = tag_text.as_bytes()[vs];
                        let mut end = vs + 1;
                        while end < tag_text.len() {
                            if tag_text.as_bytes()[end] == b'\\' {
                                end += 2;
                                if end >= tag_text.len() {
                                    break;
                                }
                                continue;
                            }
                            if tag_text.as_bytes()[end] == quote {
                                end += 1;
                                break;
                            }
                            end += 1;
                        }
                        end
                    } else {
                        // Bare word — find end
                        let mut end = vs;
                        while end < tag_text.len() {
                            let b = tag_text.as_bytes()[end];
                            if b.is_ascii_whitespace() || b == b',' || b == b')' {
                                break;
                            }
                            end += 1;
                        }
                        end
                    };

                    let mut result = String::with_capacity(tag_text.len());
                    result.push_str(&tag_text[..vs]);
                    result.push_str(new_value);
                    result.push_str(&tag_text[value_end..]);
                    return Ok(result);
                }
            }
            search_pos = abs_idx + 1;
        } else {
            break;
        }
    }

    Err(RagtagError::ParseError {
        file: "".into(),
        line: 0,
        message: format!("could not find attribute \"{attr_name}\" in tag text"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_modify_existing_attribute() {
        let result = modify_tag_attribute("@task(status=\"old\")", "status", "\"new\"").unwrap();
        assert!(result.contains("\"new\""));
        assert!(!result.contains("\"old\""));
    }

    #[test]
    fn test_insert_new_attribute() {
        let result = modify_tag_attribute("@task(id=\"abc\")", "status", "\"new\"").unwrap();
        assert!(result.contains("status=\"new\""));
        assert!(result.contains("id=\"abc\""));
    }

    #[test]
    fn test_multiline_tag_edit() {
        let tag = "@task(\n    id=\"abc\",\n    status=\"old\"\n)";
        let result = modify_tag_attribute(tag, "status", "\"done\"").unwrap();
        assert!(result.contains("\"done\""));
        assert!(!result.contains("\"old\""));
    }

    #[test]
    fn test_trailing_backslash_no_panic() {
        // Malformed tag with a trailing backslash should not panic.
        let tag = r#"@task(title="hello\")"#;
        // The value `"hello\"` has a trailing backslash that escapes the
        // closing quote, so the parser never finds the end-quote.
        // This must not panic — returning an error or a best-effort
        // result is acceptable.
        let result = modify_tag_attribute(tag, "title", "\"world\"");
        // We only care that it didn't panic. Either Ok or Err is fine.
        let _ = result;
    }

    #[test]
    fn test_atomic_file_edit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "before @task(status=\"old\") after").unwrap();

        let editor = AtomicFileEditor;
        // Tag span for "@task(status=\"old\")"
        let content = fs::read_to_string(&file).unwrap();
        let tag_start = content.find("@task").unwrap();
        let tag_end = content.find(')').unwrap() + 1;

        editor
            .update_tag_attribute(&file, tag_start..tag_end, "status", "\"done\"")
            .unwrap();

        let updated = fs::read_to_string(&file).unwrap();
        assert!(updated.contains("\"done\""));
        assert!(updated.contains("before"));
        assert!(updated.contains("after"));
    }

    #[test]
    fn test_permissions_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "@task(status=\"old\")").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        }

        let original_perms = fs::metadata(&file).unwrap().permissions();

        let editor = AtomicFileEditor;
        editor
            .update_tag_attribute(&file, 0..19, "status", "\"new\"")
            .unwrap();

        let new_perms = fs::metadata(&file).unwrap().permissions();
        assert_eq!(original_perms, new_perms);
    }

    #[test]
    fn test_symlink_rejection() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        fs::write(&real, "@task(status=\"old\")").unwrap();
        let link = dir.path().join("link.txt");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let editor = AtomicFileEditor;
            let result = editor.update_tag_attribute(&link, 0..19, "status", "\"new\"");
            assert!(matches!(result, Err(RagtagError::SymlinkEdit(_))));
        }
    }

    #[test]
    fn test_content_preservation() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        let content = "Line 1\nLine 2\n@task(status=\"old\")\nLine 4\n";
        fs::write(&file, content).unwrap();

        let tag_start = content.find("@task").unwrap();
        let tag_end = content[tag_start..].find(')').unwrap() + tag_start + 1;

        let editor = AtomicFileEditor;
        editor
            .update_tag_attribute(&file, tag_start..tag_end, "status", "\"new\"")
            .unwrap();

        let updated = fs::read_to_string(&file).unwrap();
        assert!(updated.contains("Line 1"));
        assert!(updated.contains("Line 2"));
        assert!(updated.contains("Line 4"));
        assert!(updated.contains("\"new\""));
    }

    #[test]
    fn test_multiple_tags_only_target_modified() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        let content = "@tag1(status=\"keep\") text @tag2(status=\"old\") more";
        fs::write(&file, content).unwrap();

        // Target the second tag
        let tag2_start = content.find("@tag2").unwrap();
        let tag2_end = content[tag2_start..].find(')').unwrap() + tag2_start + 1;

        let editor = AtomicFileEditor;
        editor
            .update_tag_attribute(&file, tag2_start..tag2_end, "status", "\"new\"")
            .unwrap();

        let updated = fs::read_to_string(&file).unwrap();
        // First tag should be unchanged
        assert!(updated.contains("@tag1(status=\"keep\")"));
        // Second tag should be modified
        assert!(updated.contains("\"new\""));
    }

    #[test]
    fn test_hyphenated_attr_name_not_matched() {
        // "my-status" should NOT match when searching for "status"
        let tag = "@task(my-status=\"old\", status=\"active\")";
        let result = replace_attribute_value(tag, "status", "\"done\"").unwrap();
        // "my-status" should still be "old"
        assert!(result.contains("my-status=\"old\""));
        // "status" (the standalone one) should be "done"
        assert!(result.contains("status=\"done\""));
    }

    #[test]
    fn test_invalid_span_out_of_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "@task(status=\"old\")").unwrap();

        let editor = AtomicFileEditor;
        // Span extends past end of file
        let result = editor.update_tag_attribute(&file, 0..999, "status", "\"new\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_span_reversed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "@task(status=\"old\")").unwrap();

        let editor = AtomicFileEditor;
        // start > end
        let result = editor.update_tag_attribute(&file, 10..5, "status", "\"new\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_modify_value_with_comma() {
        let tag = r#"@task(id="abc123", description="old")"#;
        let result = modify_tag_attribute(tag, "description", "\"First, second\"").unwrap();
        assert!(result.contains("description=\"First, second\""));
        // Other attributes must remain intact.
        assert!(result.contains("id=\"abc123\""));
    }

    #[test]
    fn test_modify_value_with_parentheses() {
        let tag = r#"@task(id="abc123", title="old")"#;
        let result = modify_tag_attribute(tag, "title", "\"Fix bug (urgent)\"").unwrap();
        assert!(result.contains("title=\"Fix bug (urgent)\""));
        assert!(result.contains("id=\"abc123\""));
    }
}
