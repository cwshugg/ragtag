//! Tag location tracking within source files.
//!
//! `TagLocation` records where a tag was found: file path, line number,
//! column number, and byte offsets. All line/column numbers are 1-based.

use std::path::PathBuf;

/// Describes the exact position of a tag within a source file.
#[derive(Debug, Clone, PartialEq)]
pub struct TagLocation {
    /// The file path where this tag was found.
    pub file_path: PathBuf,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub column: usize,
    /// 0-based byte offset of the `@` character.
    pub byte_offset: usize,
    /// 0-based byte offset of the end of the tag (exclusive).
    pub byte_end: usize,
}

impl TagLocation {
    /// Creates a new `TagLocation`.
    pub fn new(
        file_path: PathBuf,
        line: usize,
        column: usize,
        byte_offset: usize,
        byte_end: usize,
    ) -> Self {
        Self {
            file_path,
            line,
            column,
            byte_offset,
            byte_end,
        }
    }
}

impl std::fmt::Display for TagLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file_path.display(), self.line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_construction() {
        let loc = TagLocation::new(PathBuf::from("test.md"), 5, 3, 42, 60);
        assert_eq!(loc.file_path, PathBuf::from("test.md"));
        assert_eq!(loc.line, 5);
        assert_eq!(loc.column, 3);
        assert_eq!(loc.byte_offset, 42);
        assert_eq!(loc.byte_end, 60);
    }

    #[test]
    fn test_location_clone() {
        let loc = TagLocation::new(PathBuf::from("a.txt"), 1, 1, 0, 5);
        let loc2 = loc.clone();
        assert_eq!(loc, loc2);
    }

    #[test]
    fn test_location_debug() {
        let loc = TagLocation::new(PathBuf::from("a.txt"), 1, 1, 0, 5);
        let debug = format!("{:?}", loc);
        assert!(debug.contains("TagLocation"));
    }

    #[test]
    fn test_location_display() {
        let loc = TagLocation::new(PathBuf::from("src/main.rs"), 42, 5, 100, 150);
        assert_eq!(format!("{loc}"), "src/main.rs:42");
    }
}
