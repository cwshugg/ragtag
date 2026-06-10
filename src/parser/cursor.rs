//! Cursor for tracking position through input text.
//!
//! The `Cursor` is the foundational building block of the hand-rolled parser.
//! It tracks the current byte position, line number, and column number
//! as it advances character-by-character through a `&str` input.

/// A lightweight snapshot of cursor state, used for backtracking.
#[derive(Debug, Clone, Copy)]
pub struct CursorState {
    pub pos: usize,
    pub line: usize,
    pub col: usize,
}

/// Tracks position through input text for the parser.
///
/// Maintains byte offset, 1-based line, and 1-based column counts
/// incrementally as characters are consumed.
#[derive(Debug)]
pub struct Cursor<'a> {
    /// The full input text.
    input: &'a str,
    /// Current byte offset (0-based).
    pub pos: usize,
    /// Current line number (1-based).
    pub line: usize,
    /// Current column number (1-based).
    pub col: usize,
}

impl<'a> Cursor<'a> {
    /// Creates a new cursor at the beginning of the input.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    /// Returns the full input text.
    pub fn input(&self) -> &'a str {
        self.input
    }

    /// Looks at the current character without advancing.
    pub fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    /// Looks at the current byte without advancing.
    pub fn peek_byte(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    /// Consumes one character, updating position, line, and column.
    ///
    /// When a `\n` is consumed, `line` increments and `col` resets to 1.
    /// Otherwise, `col` increments by the character's UTF-8 byte length.
    pub fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        let byte_len = ch.len_utf8();
        self.pos += byte_len;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += byte_len;
        }
        Some(ch)
    }

    /// Returns `true` if the cursor is at or past the end of input.
    pub fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Returns the remaining unprocessed input from the current position.
    pub fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    /// Extracts a byte range from the full input.
    pub fn slice(&self, start: usize, end: usize) -> &'a str {
        &self.input[start..end]
    }

    /// Saves the current cursor state for potential backtracking.
    pub fn save(&self) -> CursorState {
        CursorState {
            pos: self.pos,
            line: self.line,
            col: self.col,
        }
    }

    /// Restores a previously saved cursor state.
    pub fn restore(&mut self, state: CursorState) {
        self.pos = state.pos;
        self.line = state.line;
        self.col = state.col;
    }
}

/// Consumes all whitespace characters (space, tab, newline, carriage return)
/// at the current position.
pub fn skip_whitespace(cursor: &mut Cursor) {
    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_whitespace() {
            cursor.advance();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_cursor() {
        let cursor = Cursor::new("hello");
        assert_eq!(cursor.pos, 0);
        assert_eq!(cursor.line, 1);
        assert_eq!(cursor.col, 1);
    }

    #[test]
    fn test_advance_ascii() {
        let mut cursor = Cursor::new("abc");
        assert_eq!(cursor.advance(), Some('a'));
        assert_eq!(cursor.pos, 1);
        assert_eq!(cursor.col, 2);
        assert_eq!(cursor.advance(), Some('b'));
        assert_eq!(cursor.advance(), Some('c'));
        assert!(cursor.is_eof());
        assert_eq!(cursor.advance(), None);
    }

    #[test]
    fn test_line_column_tracking() {
        let mut cursor = Cursor::new("ab\ncd\n");
        cursor.advance(); // a -> col=2
        cursor.advance(); // b -> col=3
        cursor.advance(); // \n -> line=2, col=1
        assert_eq!(cursor.line, 2);
        assert_eq!(cursor.col, 1);
        cursor.advance(); // c -> col=2
        cursor.advance(); // d -> col=3
        cursor.advance(); // \n -> line=3, col=1
        assert_eq!(cursor.line, 3);
        assert_eq!(cursor.col, 1);
    }

    #[test]
    fn test_crlf_handling() {
        let mut cursor = Cursor::new("a\r\nb");
        cursor.advance(); // a
        cursor.advance(); // \r -> col=3
        assert_eq!(cursor.line, 1);
        cursor.advance(); // \n -> line=2, col=1
        assert_eq!(cursor.line, 2);
        assert_eq!(cursor.col, 1);
        cursor.advance(); // b
        assert_eq!(cursor.col, 2);
    }

    #[test]
    fn test_utf8_multibyte() {
        let mut cursor = Cursor::new("é€");
        cursor.advance(); // é is 2 bytes
        assert_eq!(cursor.pos, 2);
        assert_eq!(cursor.col, 3); // col increments by byte length
        cursor.advance(); // € is 3 bytes
        assert_eq!(cursor.pos, 5);
    }

    #[test]
    fn test_save_restore() {
        let mut cursor = Cursor::new("hello world");
        cursor.advance();
        cursor.advance();
        let state = cursor.save();
        cursor.advance();
        cursor.advance();
        assert_eq!(cursor.pos, 4);
        cursor.restore(state);
        assert_eq!(cursor.pos, 2);
        assert_eq!(cursor.peek(), Some('l'));
    }

    #[test]
    fn test_peek_at_eof() {
        let cursor = Cursor::new("");
        assert_eq!(cursor.peek(), None);
        assert_eq!(cursor.peek_byte(), None);
        assert!(cursor.is_eof());
    }

    #[test]
    fn test_remaining() {
        let mut cursor = Cursor::new("hello");
        cursor.advance();
        cursor.advance();
        assert_eq!(cursor.remaining(), "llo");
    }

    #[test]
    fn test_slice() {
        let cursor = Cursor::new("hello world");
        assert_eq!(cursor.slice(0, 5), "hello");
        assert_eq!(cursor.slice(6, 11), "world");
    }

    #[test]
    fn test_skip_whitespace() {
        let mut cursor = Cursor::new("   \t\nhello");
        skip_whitespace(&mut cursor);
        assert_eq!(cursor.peek(), Some('h'));
        assert_eq!(cursor.line, 2);
    }

    #[test]
    fn test_skip_whitespace_no_ws() {
        let mut cursor = Cursor::new("hello");
        skip_whitespace(&mut cursor);
        assert_eq!(cursor.pos, 0);
    }
}
