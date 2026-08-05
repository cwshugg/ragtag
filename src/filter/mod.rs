//! Shared boolean filter-expression engine.
//!
//! This module provides the domain-agnostic machinery used by every `--filter`
//! flag in ragtag: an AST (`FilterExpr`), a whitespace-tolerant tokenizer, a
//! recursive-descent parser, and generic evaluation/validation that defer each
//! leaf condition to a caller-supplied predicate. The engine knows nothing
//! about tasks, tags, or any particular attribute set — callers supply a leaf
//! closure that gives condition strings their meaning.
//!
//! # Grammar
//!
//! ```text
//! expr   → term (OR term)*
//! term   → factor (AND factor)*
//! factor → '(' expr ')' | condition
//! ```
//!
//! AND binds tighter than OR (standard boolean precedence). Parentheses
//! override precedence. A `condition` is a `field <op> value` triple using one
//! of the comparison operators `=`, `!=`, `>`, `<`, `>=`, `<=`. Optional
//! whitespace is allowed around a condition's operator (e.g. `status = active`
//! and `status=active` are equivalent). Values may be single- or double-quoted
//! to include spaces (e.g. `owner='John Doe'`). AND/OR keywords are
//! case-insensitive.
//!
//! # Examples
//!
//! ```text
//! status=active
//! status = active
//! status=active AND priority<=2
//! (status = active OR status=blocked) AND owner=alice
//! owner='John Doe' AND status!=done
//! ```

use crate::error::RagtagError;
use std::iter::Peekable;
use std::str::Chars;

/// A parsed filter expression tree.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterExpr {
    /// A leaf condition (e.g., `status=active`, `priority>2`).
    Condition(String),
    /// Logical AND of two sub-expressions.
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical OR of two sub-expressions.
    Or(Box<FilterExpr>, Box<FilterExpr>),
}

/// A token produced by the tokenizer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    LParen,
    RParen,
    And,
    Or,
    Condition(String),
}

/// Returns true if `c` can begin a comparison operator.
fn is_operator_start(c: char) -> bool {
    matches!(c, '=' | '!' | '<' | '>')
}

/// Skips over any consecutive whitespace at the front of `chars`.
fn skip_whitespace(chars: &mut Peekable<Chars>) {
    while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
        chars.next();
    }
}

/// Reads a quoted string, including its surrounding quotes.
///
/// Assumes the next character in `chars` is the opening quote. Returns an
/// error if the matching closing quote is never found.
fn read_quoted(chars: &mut Peekable<Chars>, input: &str) -> Result<String, RagtagError> {
    let quote = chars.next().expect("caller guarantees an opening quote");
    let mut out = String::new();
    out.push(quote);
    for qch in chars.by_ref() {
        out.push(qch);
        if qch == quote {
            return Ok(out);
        }
    }
    Err(RagtagError::InvalidFilter(format!(
        "unterminated quote in filter expression: \"{input}\""
    )))
}

/// Reads a bare (unquoted) run of characters that forms a field name, an
/// operator-less word, or a value.
///
/// Stops at whitespace, parentheses, an operator character, or a quote, none
/// of which belong to the run being read.
fn read_bare(chars: &mut Peekable<Chars>) -> String {
    let mut out = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace()
            || c == '('
            || c == ')'
            || is_operator_start(c)
            || c == '\''
            || c == '"'
        {
            break;
        }
        out.push(c);
        chars.next();
    }
    out
}

/// Reads a comparison operator (`=`, `!=`, `>`, `<`, `>=`, `<=`).
///
/// Assumes the next character in `chars` begins an operator.
fn read_operator(chars: &mut Peekable<Chars>) -> String {
    let mut op = String::new();
    if let Some(c) = chars.next() {
        op.push(c);
        if matches!(c, '<' | '>' | '!') && matches!(chars.peek(), Some('=')) {
            op.push('=');
            chars.next();
        }
    }
    op
}

/// Reads the next non-parenthesis token: either an `AND`/`OR` keyword or a
/// full `field <op> value` condition.
///
/// Whitespace is permitted (but not required) around the operator and between
/// the operator and its value; the emitted condition is normalized to the
/// canonical `field<op>value` form. A bare word is treated as a keyword only
/// when it is not followed by an operator, so a value is never mistaken for a
/// keyword.
fn read_keyword_or_condition(
    chars: &mut Peekable<Chars>,
    input: &str,
) -> Result<Token, RagtagError> {
    // A leading quote has no field or operator; keep it whole so later
    // validation can reject the malformed condition.
    if matches!(chars.peek(), Some('\'' | '"')) {
        return Ok(Token::Condition(read_quoted(chars, input)?));
    }

    let field = read_bare(chars);
    skip_whitespace(chars);

    if matches!(chars.peek(), Some(&c) if is_operator_start(c)) {
        let op = read_operator(chars);
        skip_whitespace(chars);
        let value = match chars.peek() {
            Some('\'' | '"') => read_quoted(chars, input)?,
            _ => read_bare(chars),
        };
        // An empty value after the operator (e.g. `field=`, `field!=`) is
        // permitted: leaf evaluators compare it against the empty string, so a
        // condition like `worktime_spent=` matches tasks whose field is empty
        // or absent. A bare word with no operator at all is handled below and
        // rejected later by the per-leaf operator validation.
        Ok(Token::Condition(format!("{field}{op}{value}")))
    } else {
        match field.to_uppercase().as_str() {
            "AND" => Ok(Token::And),
            "OR" => Ok(Token::Or),
            _ => Ok(Token::Condition(field)),
        }
    }
}

/// Tokenizes a filter expression string into a sequence of tokens.
///
/// Handles:
/// - Parentheses (even without surrounding whitespace)
/// - Optional whitespace around comparison operators within a condition
/// - Quoted values (single or double quotes) to allow spaces in values
/// - Case-insensitive AND/OR operators
fn tokenize(input: &str) -> Result<Vec<Token>, RagtagError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    loop {
        skip_whitespace(&mut chars);
        let Some(&ch) = chars.peek() else { break };

        match ch {
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            _ => tokens.push(read_keyword_or_condition(&mut chars, input)?),
        }
    }

    Ok(tokens)
}

/// Parses a filter expression string into a `FilterExpr` AST.
///
/// Returns an error if the expression is empty, has unmatched parentheses,
/// or is otherwise malformed.
pub fn parse_filter_expr(input: &str) -> Result<FilterExpr, RagtagError> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err(RagtagError::InvalidFilter(
            "empty filter expression".to_string(),
        ));
    }
    let mut pos = 0;
    let expr = parse_expr(&tokens, &mut pos)?;
    if pos != tokens.len() {
        return Err(RagtagError::InvalidFilter(format!(
            "unexpected token at position {pos} in filter expression: \"{input}\""
        )));
    }
    Ok(expr)
}

/// Parses an `expr` (OR level — lowest precedence).
///
/// ```text
/// expr → term (OR term)*
/// ```
fn parse_expr(tokens: &[Token], pos: &mut usize) -> Result<FilterExpr, RagtagError> {
    let mut left = parse_term(tokens, pos)?;
    while *pos < tokens.len() && tokens[*pos] == Token::Or {
        *pos += 1; // consume OR
        let right = parse_term(tokens, pos)?;
        left = FilterExpr::Or(Box::new(left), Box::new(right));
    }
    Ok(left)
}

/// Parses a `term` (AND level — higher precedence than OR).
///
/// ```text
/// term → factor (AND factor)*
/// ```
fn parse_term(tokens: &[Token], pos: &mut usize) -> Result<FilterExpr, RagtagError> {
    let mut left = parse_factor(tokens, pos)?;
    while *pos < tokens.len() && tokens[*pos] == Token::And {
        *pos += 1; // consume AND
        let right = parse_factor(tokens, pos)?;
        left = FilterExpr::And(Box::new(left), Box::new(right));
    }
    Ok(left)
}

/// Parses a `factor` (parenthesized expression or leaf condition).
///
/// ```text
/// factor → '(' expr ')' | condition
/// ```
fn parse_factor(tokens: &[Token], pos: &mut usize) -> Result<FilterExpr, RagtagError> {
    if *pos >= tokens.len() {
        return Err(RagtagError::InvalidFilter(
            "unexpected end of filter expression".to_string(),
        ));
    }

    match &tokens[*pos] {
        Token::LParen => {
            *pos += 1; // consume '('
            let expr = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() || tokens[*pos] != Token::RParen {
                return Err(RagtagError::InvalidFilter(
                    "unmatched '(' in filter expression".to_string(),
                ));
            }
            *pos += 1; // consume ')'
            Ok(expr)
        }
        Token::Condition(cond) => {
            // The condition token keeps its value quotes; the shared
            // `split_condition` helper (used by each leaf) locates the operator
            // outside quoted spans and strips the value quotes exactly once.
            let expr = FilterExpr::Condition(cond.clone());
            *pos += 1;
            Ok(expr)
        }
        Token::RParen => Err(RagtagError::InvalidFilter(
            "unexpected ')' in filter expression".to_string(),
        )),
        Token::And => Err(RagtagError::InvalidFilter(
            "unexpected 'AND' at start of expression".to_string(),
        )),
        Token::Or => Err(RagtagError::InvalidFilter(
            "unexpected 'OR' at start of expression".to_string(),
        )),
    }
}

/// Locates the comparison operator that separates a condition's field from its
/// value, ignoring operator characters that appear inside a quoted span.
///
/// Returns the byte offset of the operator and the operator itself. The scan
/// runs left to right and matches the longest valid operator (`!=`, `>=`, `<=`
/// before `>`, `<`, `=`) at the first operator position outside quotes; a bare
/// `!` is not a valid operator and is skipped. Because a field name never
/// contains an operator character, the first operator found outside quotes is
/// always the true field/value separator, even when the value later contains
/// operator characters (e.g. an unquoted `label=>2`).
fn find_operator(condition: &str) -> Option<(usize, &'static str)> {
    let mut in_quote: Option<char> = None;
    for (i, c) in condition.char_indices() {
        match in_quote {
            Some(quote) => {
                if c == quote {
                    in_quote = None;
                }
            }
            None => {
                if c == '\'' || c == '"' {
                    in_quote = Some(c);
                } else if is_operator_start(c) {
                    let rest = &condition[i..];
                    let op = if let Some(op) = ["!=", ">=", "<="]
                        .into_iter()
                        .find(|op| rest.starts_with(op))
                    {
                        op
                    } else if let Some(op) =
                        [">", "<", "="].into_iter().find(|op| rest.starts_with(op))
                    {
                        op
                    } else {
                        // A lone `!` is not a comparison operator; keep scanning.
                        continue;
                    };
                    return Some((i, op));
                }
            }
        }
    }
    None
}

/// Splits a single condition into its `(field, operator, value)` parts.
///
/// The operator is located with [`find_operator`], so operator characters
/// inside a quoted value are ignored. The field and value are trimmed, and the
/// value has its surrounding quotes removed (e.g. `owner='John Doe'` yields
/// `("owner", "=", "John Doe")`). Returns `None` when the condition contains no
/// comparison operator outside quotes.
///
/// Both the task and query leaves use this helper so their field/value split is
/// identical; each applies its own per-domain comparison semantics to the
/// result.
pub fn split_condition(condition: &str) -> Option<(&str, &str, &str)> {
    let (idx, op) = find_operator(condition)?;
    let field = condition[..idx].trim();
    let value = strip_quotes(condition[idx + op.len()..].trim());
    Some((field, op, value))
}

/// Verifies that a condition contains a comparison operator outside any quoted
/// span, returning a single canonical error otherwise.
///
/// This is the shared "has a valid operator" check used by every leaf
/// validator so malformed conditions surface one consistent message.
pub fn ensure_condition_has_operator(condition: &str) -> Result<(), RagtagError> {
    if split_condition(condition).is_some() {
        Ok(())
    } else {
        Err(RagtagError::InvalidFilter(format!(
            "\"{condition}\" — expected format: field=value, field!=value, field>value, etc."
        )))
    }
}

/// Strips surrounding quotes (single or double) from a string.
fn strip_quotes(s: &str) -> &str {
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        if (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
        {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// Evaluates a parsed filter expression, deferring each leaf condition to
/// `leaf`.
///
/// The `leaf` predicate is called with each `Condition` string (in canonical
/// `field<op>value` form) and returns whether that condition holds. AND and OR
/// nodes combine the leaf results with standard boolean logic.
pub fn evaluate(expr: &FilterExpr, leaf: &mut impl FnMut(&str) -> bool) -> bool {
    match expr {
        FilterExpr::Condition(cond) => leaf(cond),
        FilterExpr::And(left, right) => evaluate(left, leaf) && evaluate(right, leaf),
        FilterExpr::Or(left, right) => evaluate(left, leaf) || evaluate(right, leaf),
    }
}

/// Validates every leaf condition in a filter expression using `leaf`.
///
/// The `leaf` validator is called with each `Condition` string and returns an
/// error for any malformed condition. Validation stops at the first error.
pub fn validate(
    expr: &FilterExpr,
    leaf: &impl Fn(&str) -> Result<(), RagtagError>,
) -> Result<(), RagtagError> {
    match expr {
        FilterExpr::Condition(cond) => leaf(cond),
        FilterExpr::And(left, right) | FilterExpr::Or(left, right) => {
            validate(left, leaf)?;
            validate(right, leaf)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // Tokenizer tests
    // =====================================================================

    #[test]
    fn test_tokenize_simple_condition() {
        let tokens = tokenize("status=active").unwrap();
        assert_eq!(tokens, vec![Token::Condition("status=active".to_string())]);
    }

    #[test]
    fn test_tokenize_and_expression() {
        let tokens = tokenize("status=active AND priority=0").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Condition("status=active".to_string()),
                Token::And,
                Token::Condition("priority=0".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_or_expression() {
        let tokens = tokenize("status=active OR status=blocked").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Condition("status=active".to_string()),
                Token::Or,
                Token::Condition("status=blocked".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_parentheses() {
        let tokens = tokenize("(status=active OR status=blocked)").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::LParen,
                Token::Condition("status=active".to_string()),
                Token::Or,
                Token::Condition("status=blocked".to_string()),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn test_tokenize_complex_expression() {
        let tokens = tokenize("(status=active OR priority>2) AND owner=me").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::LParen,
                Token::Condition("status=active".to_string()),
                Token::Or,
                Token::Condition("priority>2".to_string()),
                Token::RParen,
                Token::And,
                Token::Condition("owner=me".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_case_insensitive_operators() {
        let tokens = tokenize("a=1 and b=2 Or c=3 AND d=4").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Condition("a=1".to_string()),
                Token::And,
                Token::Condition("b=2".to_string()),
                Token::Or,
                Token::Condition("c=3".to_string()),
                Token::And,
                Token::Condition("d=4".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_quoted_values() {
        let tokens = tokenize("owner='John Doe'").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Condition("owner='John Doe'".to_string())]
        );
    }

    #[test]
    fn test_tokenize_quoted_double() {
        let tokens = tokenize("title=\"My Task\"").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Condition("title=\"My Task\"".to_string())]
        );
    }

    #[test]
    fn test_tokenize_paren_no_space() {
        // Parentheses without surrounding whitespace
        let tokens = tokenize("(status=active)AND(priority=0)").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::LParen,
                Token::Condition("status=active".to_string()),
                Token::RParen,
                Token::And,
                Token::LParen,
                Token::Condition("priority=0".to_string()),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn test_tokenize_unterminated_quote() {
        let result = tokenize("owner='John Doe");
        assert!(result.is_err());
    }

    // =====================================================================
    // Tokenizer tests: whitespace around operators
    // =====================================================================

    #[test]
    fn test_tokenize_spaces_around_equals() {
        // Symmetric and asymmetric spacing all normalize to the same token.
        let expected = vec![Token::Condition("status=active".to_string())];
        assert_eq!(tokenize("status = active").unwrap(), expected);
        assert_eq!(tokenize("status =active").unwrap(), expected);
        assert_eq!(tokenize("status= active").unwrap(), expected);
        assert_eq!(tokenize("status=active").unwrap(), expected);
    }

    #[test]
    fn test_tokenize_spaces_around_each_operator() {
        assert_eq!(
            tokenize("priority != 0").unwrap(),
            vec![Token::Condition("priority!=0".to_string())]
        );
        assert_eq!(
            tokenize("priority >= 2").unwrap(),
            vec![Token::Condition("priority>=2".to_string())]
        );
        assert_eq!(
            tokenize("priority <= 2").unwrap(),
            vec![Token::Condition("priority<=2".to_string())]
        );
        assert_eq!(
            tokenize("priority > 2").unwrap(),
            vec![Token::Condition("priority>2".to_string())]
        );
        assert_eq!(
            tokenize("priority < 2").unwrap(),
            vec![Token::Condition("priority<2".to_string())]
        );
    }

    #[test]
    fn test_tokenize_spaced_quoted_value() {
        let tokens = tokenize("owner = 'John Doe'").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Condition("owner='John Doe'".to_string())]
        );
    }

    #[test]
    fn test_tokenize_spaced_full_user_expression() {
        let tokens = tokenize("(status = active OR priority = 0) AND status != done").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::LParen,
                Token::Condition("status=active".to_string()),
                Token::Or,
                Token::Condition("priority=0".to_string()),
                Token::RParen,
                Token::And,
                Token::Condition("status!=done".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_mixed_spaced_and_spaceless() {
        let tokens = tokenize("status=active AND priority >= 2").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Condition("status=active".to_string()),
                Token::And,
                Token::Condition("priority>=2".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_value_that_looks_like_keyword() {
        // A bare word followed by an operator is a field, never a keyword.
        let tokens = tokenize("and = or").unwrap();
        assert_eq!(tokens, vec![Token::Condition("and=or".to_string())]);
    }

    #[test]
    fn test_tokenize_backward_compatible_spaceless() {
        // Existing spaceless expressions tokenize exactly as before.
        let tokens = tokenize("status=active AND priority=0").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Condition("status=active".to_string()),
                Token::And,
                Token::Condition("priority=0".to_string()),
            ]
        );
    }

    #[test]
    fn test_tokenize_empty_value_is_accepted() {
        // An empty value after an operator is permitted; leaves compare it
        // against the empty string. Trailing whitespace is ignored.
        let expected = vec![Token::Condition("status=".to_string())];
        assert_eq!(tokenize("status =").unwrap(), expected);
        assert_eq!(tokenize("status = ").unwrap(), expected);
        assert_eq!(tokenize("status=").unwrap(), expected);
        assert_eq!(
            tokenize("worktime_spent!=").unwrap(),
            vec![Token::Condition("worktime_spent!=".to_string())]
        );
    }

    // =====================================================================
    // Parser tests
    // =====================================================================

    #[test]
    fn test_parse_single_condition() {
        let expr = parse_filter_expr("status=active").unwrap();
        assert_eq!(expr, FilterExpr::Condition("status=active".to_string()));
    }

    #[test]
    fn test_parse_and_expression() {
        let expr = parse_filter_expr("status=active AND priority=0").unwrap();
        assert_eq!(
            expr,
            FilterExpr::And(
                Box::new(FilterExpr::Condition("status=active".to_string())),
                Box::new(FilterExpr::Condition("priority=0".to_string())),
            )
        );
    }

    #[test]
    fn test_parse_or_expression() {
        let expr = parse_filter_expr("status=active OR status=blocked").unwrap();
        assert_eq!(
            expr,
            FilterExpr::Or(
                Box::new(FilterExpr::Condition("status=active".to_string())),
                Box::new(FilterExpr::Condition("status=blocked".to_string())),
            )
        );
    }

    #[test]
    fn test_parse_and_or_precedence() {
        // AND binds tighter than OR: a OR b AND c == a OR (b AND c)
        let expr = parse_filter_expr("a=1 OR b=2 AND c=3").unwrap();
        assert_eq!(
            expr,
            FilterExpr::Or(
                Box::new(FilterExpr::Condition("a=1".to_string())),
                Box::new(FilterExpr::And(
                    Box::new(FilterExpr::Condition("b=2".to_string())),
                    Box::new(FilterExpr::Condition("c=3".to_string())),
                )),
            )
        );
    }

    #[test]
    fn test_parse_parentheses_override_precedence() {
        // (a OR b) AND c
        let expr = parse_filter_expr("(a=1 OR b=2) AND c=3").unwrap();
        assert_eq!(
            expr,
            FilterExpr::And(
                Box::new(FilterExpr::Or(
                    Box::new(FilterExpr::Condition("a=1".to_string())),
                    Box::new(FilterExpr::Condition("b=2".to_string())),
                )),
                Box::new(FilterExpr::Condition("c=3".to_string())),
            )
        );
    }

    #[test]
    fn test_parse_nested_parentheses() {
        let expr = parse_filter_expr("((a=1))").unwrap();
        assert_eq!(expr, FilterExpr::Condition("a=1".to_string()));
    }

    #[test]
    fn test_parse_error_unmatched_paren() {
        assert!(parse_filter_expr("(a=1").is_err());
        assert!(parse_filter_expr("a=1)").is_err());
    }

    #[test]
    fn test_parse_error_empty_expression() {
        assert!(parse_filter_expr("").is_err());
        assert!(parse_filter_expr("   ").is_err());
    }

    #[test]
    fn test_parse_quoted_value_kept_until_leaf() {
        // The AST keeps the value's quotes; the shared splitter strips them
        // once when a leaf evaluates the condition.
        let expr = parse_filter_expr("owner='John Doe'").unwrap();
        assert_eq!(expr, FilterExpr::Condition("owner='John Doe'".to_string()));
        assert_eq!(
            split_condition("owner='John Doe'"),
            Some(("owner", "=", "John Doe"))
        );
    }

    #[test]
    fn test_parse_spaced_operator_matches_spaceless() {
        // Every spacing variant parses to the same canonical AST.
        let canonical = FilterExpr::Condition("status=active".to_string());
        assert_eq!(parse_filter_expr("status = active").unwrap(), canonical);
        assert_eq!(parse_filter_expr("status =active").unwrap(), canonical);
        assert_eq!(parse_filter_expr("status= active").unwrap(), canonical);
        assert_eq!(parse_filter_expr("status=active").unwrap(), canonical);
    }

    #[test]
    fn test_parse_spaced_quoted_value_kept_until_leaf() {
        let expr = parse_filter_expr("owner = 'John Doe'").unwrap();
        assert_eq!(expr, FilterExpr::Condition("owner='John Doe'".to_string()));
        assert_eq!(
            split_condition("owner='John Doe'"),
            Some(("owner", "=", "John Doe"))
        );
    }

    // =====================================================================
    // Shared single-condition splitter tests
    // =====================================================================

    #[test]
    fn test_split_condition_each_operator() {
        assert_eq!(
            split_condition("status=active"),
            Some(("status", "=", "active"))
        );
        assert_eq!(
            split_condition("status!=done"),
            Some(("status", "!=", "done"))
        );
        assert_eq!(
            split_condition("priority>=2"),
            Some(("priority", ">=", "2"))
        );
        assert_eq!(
            split_condition("priority<=2"),
            Some(("priority", "<=", "2"))
        );
        assert_eq!(split_condition("priority>2"), Some(("priority", ">", "2")));
        assert_eq!(split_condition("priority<2"), Some(("priority", "<", "2")));
    }

    #[test]
    fn test_split_condition_empty_value() {
        assert_eq!(
            split_condition("worktime_spent="),
            Some(("worktime_spent", "=", ""))
        );
        assert_eq!(split_condition("pid!="), Some(("pid", "!=", "")));
    }

    #[test]
    fn test_split_condition_no_operator() {
        assert_eq!(split_condition("nooperator"), None);
        // A lone `!` is not a comparison operator.
        assert_eq!(split_condition("a!b"), None);
    }

    #[test]
    fn test_split_condition_strips_value_quotes() {
        assert_eq!(
            split_condition("owner='John Doe'"),
            Some(("owner", "=", "John Doe"))
        );
        assert_eq!(
            split_condition("title=\"My Task\""),
            Some(("title", "=", "My Task"))
        );
    }

    #[test]
    fn test_split_condition_quoted_operator_char_ignored() {
        // Operator characters inside quotes must not be treated as the split
        // point, and the quotes are stripped from the resulting value.
        assert_eq!(split_condition("label='>2'"), Some(("label", "=", ">2")));
        assert_eq!(
            split_condition("status='!=x'"),
            Some(("status", "=", "!=x"))
        );
        assert_eq!(split_condition("owner='a=b'"), Some(("owner", "=", "a=b")));
    }

    #[test]
    fn test_parse_quoted_operator_char_splits_on_intended_operator() {
        // End-to-end: a quoted value containing an operator char keeps its
        // quotes in the AST and splits on the real operator, not the one inside
        // the quotes.
        let expr = parse_filter_expr("label='>2'").unwrap();
        assert_eq!(expr, FilterExpr::Condition("label='>2'".to_string()));
        assert_eq!(split_condition("label='>2'"), Some(("label", "=", ">2")));

        let expr = parse_filter_expr("status = '!=x'").unwrap();
        assert_eq!(expr, FilterExpr::Condition("status='!=x'".to_string()));
        assert_eq!(
            split_condition("status='!=x'"),
            Some(("status", "=", "!=x"))
        );

        let expr = parse_filter_expr("owner='a=b'").unwrap();
        assert_eq!(expr, FilterExpr::Condition("owner='a=b'".to_string()));
        assert_eq!(split_condition("owner='a=b'"), Some(("owner", "=", "a=b")));
    }

    #[test]
    fn test_ensure_condition_has_operator() {
        assert!(ensure_condition_has_operator("status=active").is_ok());
        assert!(ensure_condition_has_operator("worktime_spent=").is_ok());
        assert!(ensure_condition_has_operator("nooperator").is_err());
    }

    #[test]
    fn test_parse_empty_value_is_accepted() {
        // `field=` / `field!=` parse to a condition with an empty value; a bare
        // word swallowed as a value still leaves a dangling token that errors.
        assert_eq!(
            parse_filter_expr("status =").unwrap(),
            FilterExpr::Condition("status=".to_string())
        );
        assert!(parse_filter_expr("status = AND owner=me").is_err());
    }

    #[test]
    fn test_parse_error_missing_operator() {
        // A bare word with no operator at all remains malformed.
        let expr = parse_filter_expr("nooperator").unwrap();
        assert_eq!(expr, FilterExpr::Condition("nooperator".to_string()));
        assert!(ensure_condition_has_operator("nooperator").is_err());
    }

    #[test]
    fn test_parse_error_spaced_unmatched_paren() {
        assert!(parse_filter_expr("(status = active").is_err());
    }

    #[test]
    fn test_parse_error_spaced_unterminated_quote() {
        assert!(parse_filter_expr("owner = 'John Doe").is_err());
    }

    // =====================================================================
    // Generic evaluation/validation tests
    // =====================================================================

    #[test]
    fn test_evaluate_uses_leaf_predicate() {
        // A leaf that accepts only "a=1" drives the boolean combination.
        let mut leaf = |cond: &str| cond == "a=1";
        let expr = parse_filter_expr("a=1 AND b=2").unwrap();
        assert!(!evaluate(&expr, &mut leaf));
        let expr = parse_filter_expr("a=1 OR b=2").unwrap();
        assert!(evaluate(&expr, &mut leaf));
        let expr = parse_filter_expr("(a=1 OR b=2) AND a=1").unwrap();
        assert!(evaluate(&expr, &mut leaf));
    }

    #[test]
    fn test_validate_reports_bad_leaf() {
        // A leaf validator that rejects any condition without '=' surfaces the
        // error from the offending leaf.
        let leaf = |cond: &str| {
            if cond.contains('=') {
                Ok(())
            } else {
                Err(RagtagError::InvalidFilter(cond.to_string()))
            }
        };
        let ok = parse_filter_expr("a=1 AND b=2").unwrap();
        assert!(validate(&ok, &leaf).is_ok());
        let bad = parse_filter_expr("a=1 AND bogus").unwrap();
        assert!(validate(&bad, &leaf).is_err());
    }

    #[test]
    fn test_same_expression_parses_consistently() {
        // The shared engine yields one AST for a given string regardless of who
        // parses it, so task and query see identical structure.
        let a = parse_filter_expr("(status = active OR priority = 0) AND status != done").unwrap();
        let b = parse_filter_expr("(status=active OR priority=0) AND status!=done").unwrap();
        assert_eq!(a, b);
    }
}
