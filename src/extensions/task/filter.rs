//! Filter expression parser and evaluator.
//!
//! Supports boolean expressions with AND/OR operators and parentheses
//! for filtering tasks. Individual conditions use the same comparison
//! operators as before: `=`, `!=`, `>`, `<`, `>=`, `<=`.
//!
//! # Grammar
//!
//! ```text
//! expr   → term (OR term)*
//! term   → factor (AND factor)*
//! factor → '(' expr ')' | condition
//! ```
//!
//! AND binds tighter than OR (standard boolean precedence).
//! Parentheses override precedence.
//!
//! # Examples
//!
//! ```text
//! status=active
//! status=active AND priority<=2
//! (status=active OR status=blocked) AND owner=alice
//! owner='John Doe' AND status!=done
//! ```

use super::commands::apply_task_filter;
use super::models::TaskTag;
use crate::error::RagtagError;

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

/// Tokenizes a filter expression string into a sequence of tokens.
///
/// Handles:
/// - Parentheses (even without surrounding whitespace)
/// - Quoted values (single or double quotes) to allow spaces in values
/// - Case-insensitive AND/OR operators
fn tokenize(input: &str) -> Result<Vec<Token>, RagtagError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current = String::new();

    // Helper: flush the current token buffer into the token list.
    let flush = |current: &mut String, tokens: &mut Vec<Token>| {
        if !current.is_empty() {
            let word = std::mem::take(current);
            let upper = word.to_uppercase();
            match upper.as_str() {
                "AND" => tokens.push(Token::And),
                "OR" => tokens.push(Token::Or),
                _ => tokens.push(Token::Condition(word)),
            }
        }
    };

    while let Some(&ch) = chars.peek() {
        match ch {
            '(' => {
                flush(&mut current, &mut tokens);
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                flush(&mut current, &mut tokens);
                tokens.push(Token::RParen);
                chars.next();
            }
            '\'' | '"' => {
                // Quoted value — append everything including quotes to current
                // token so the condition string preserves the value.
                let quote = ch;
                chars.next();
                current.push(quote);
                let mut found_close = false;
                while let Some(&qch) = chars.peek() {
                    chars.next();
                    if qch == quote {
                        current.push(quote);
                        found_close = true;
                        break;
                    }
                    current.push(qch);
                }
                if !found_close {
                    return Err(RagtagError::InvalidFilter(format!(
                        "unterminated quote in filter expression: \"{input}\""
                    )));
                }
            }
            c if c.is_whitespace() => {
                flush(&mut current, &mut tokens);
                chars.next();
            }
            _ => {
                current.push(ch);
                chars.next();
            }
        }
    }

    flush(&mut current, &mut tokens);
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
            let expr = FilterExpr::Condition(strip_quotes_from_value(cond));
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

/// Strips surrounding quotes from the value portion of a condition string.
///
/// For example, `owner='John Doe'` becomes `owner=John Doe`.
/// Handles both single and double quotes on the value side only.
fn strip_quotes_from_value(condition: &str) -> String {
    // Find the operator position to split field from value.
    // Check multi-char operators first, then single-char.
    let ops = ["!=", ">=", "<=", ">", "<", "="];
    for op in ops {
        if let Some(idx) = condition.find(op) {
            let field = &condition[..idx];
            let value = &condition[idx + op.len()..];
            let stripped = strip_quotes(value);
            return format!("{field}{op}{stripped}");
        }
    }
    // No operator found — return as-is (validation will catch this later).
    condition.to_string()
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

/// Evaluates a parsed filter expression against a task.
///
/// Leaf conditions are evaluated using `apply_task_filter`.
/// AND and OR nodes combine results with standard boolean logic.
pub fn evaluate_filter(expr: &FilterExpr, task: &TaskTag) -> bool {
    match expr {
        FilterExpr::Condition(cond) => apply_task_filter(task, cond),
        FilterExpr::And(left, right) => evaluate_filter(left, task) && evaluate_filter(right, task),
        FilterExpr::Or(left, right) => evaluate_filter(left, task) || evaluate_filter(right, task),
    }
}

/// Validates all leaf conditions in a filter expression.
///
/// Returns an error if any condition is missing a comparison operator.
pub fn validate_filter_expr(expr: &FilterExpr) -> Result<(), RagtagError> {
    match expr {
        FilterExpr::Condition(cond) => super::commands::validate_task_filter(cond),
        FilterExpr::And(left, right) | FilterExpr::Or(left, right) => {
            validate_filter_expr(left)?;
            validate_filter_expr(right)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TagLocation;
    use std::path::PathBuf;

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
        // "A OR B AND C" should parse as "A OR (B AND C)"
        let expr = parse_filter_expr("status=active OR priority=0 AND owner=me").unwrap();
        assert_eq!(
            expr,
            FilterExpr::Or(
                Box::new(FilterExpr::Condition("status=active".to_string())),
                Box::new(FilterExpr::And(
                    Box::new(FilterExpr::Condition("priority=0".to_string())),
                    Box::new(FilterExpr::Condition("owner=me".to_string())),
                )),
            )
        );
    }

    #[test]
    fn test_parse_parentheses_override_precedence() {
        // "(A OR B) AND C" should group the OR first
        let expr = parse_filter_expr("(status=active OR priority>2) AND owner=me").unwrap();
        assert_eq!(
            expr,
            FilterExpr::And(
                Box::new(FilterExpr::Or(
                    Box::new(FilterExpr::Condition("status=active".to_string())),
                    Box::new(FilterExpr::Condition("priority>2".to_string())),
                )),
                Box::new(FilterExpr::Condition("owner=me".to_string())),
            )
        );
    }

    #[test]
    fn test_parse_nested_parentheses() {
        let expr = parse_filter_expr("((status=active OR status=blocked) AND owner=me)").unwrap();
        assert_eq!(
            expr,
            FilterExpr::And(
                Box::new(FilterExpr::Or(
                    Box::new(FilterExpr::Condition("status=active".to_string())),
                    Box::new(FilterExpr::Condition("status=blocked".to_string())),
                )),
                Box::new(FilterExpr::Condition("owner=me".to_string())),
            )
        );
    }

    #[test]
    fn test_parse_error_unmatched_paren() {
        assert!(parse_filter_expr("(status=active").is_err());
        assert!(parse_filter_expr("status=active)").is_err());
    }

    #[test]
    fn test_parse_error_empty_expression() {
        assert!(parse_filter_expr("").is_err());
        assert!(parse_filter_expr("   ").is_err());
    }

    #[test]
    fn test_parse_quoted_value_stripped() {
        let expr = parse_filter_expr("owner='John Doe'").unwrap();
        assert_eq!(expr, FilterExpr::Condition("owner=John Doe".to_string()));
    }

    // =====================================================================
    // Evaluation tests
    // =====================================================================

    fn make_task(id: &str, status: &str, owner: &str, priority: Option<u32>) -> TaskTag {
        TaskTag {
            id: id.to_string(),
            pid: None,
            title: format!("Task {id}"),
            description: None,
            owner: owner.to_string(),
            status: status.to_string(),
            priority,
            worktime_spent: None,
            worktime_estimate: Some(4.0),
            time_created: None,
            time_last_updated: None,
            worktime_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        }
    }

    #[test]
    fn test_eval_simple_equality() {
        let task = make_task("a", "active", "alice", Some(1));
        let expr = parse_filter_expr("status=active").unwrap();
        assert!(evaluate_filter(&expr, &task));

        let expr2 = parse_filter_expr("status=done").unwrap();
        assert!(!evaluate_filter(&expr2, &task));
    }

    #[test]
    fn test_eval_and_both_true() {
        let task = make_task("a", "active", "alice", Some(1));
        let expr = parse_filter_expr("status=active AND owner=alice").unwrap();
        assert!(evaluate_filter(&expr, &task));
    }

    #[test]
    fn test_eval_and_one_false() {
        let task = make_task("a", "active", "alice", Some(1));
        let expr = parse_filter_expr("status=active AND owner=bob").unwrap();
        assert!(!evaluate_filter(&expr, &task));
    }

    #[test]
    fn test_eval_or_one_true() {
        let task = make_task("a", "active", "alice", Some(1));
        let expr = parse_filter_expr("status=done OR owner=alice").unwrap();
        assert!(evaluate_filter(&expr, &task));
    }

    #[test]
    fn test_eval_or_both_false() {
        let task = make_task("a", "active", "alice", Some(1));
        let expr = parse_filter_expr("status=done OR owner=bob").unwrap();
        assert!(!evaluate_filter(&expr, &task));
    }

    #[test]
    fn test_eval_complex_expression() {
        let task_a = make_task("a", "active", "alice", Some(0));
        let task_b = make_task("b", "blocked", "bob", Some(3));
        let task_c = make_task("c", "inactive", "alice", Some(1));

        // (status=active OR priority>2) AND owner=alice
        let expr = parse_filter_expr("(status=active OR priority>2) AND owner=alice").unwrap();

        // task_a: status=active (true) OR priority>2 (false) = true; owner=alice = true → true
        assert!(evaluate_filter(&expr, &task_a));
        // task_b: status=active (false) OR priority>2 (true) = true; owner=alice (false) → false
        assert!(!evaluate_filter(&expr, &task_b));
        // task_c: status=active (false) OR priority>2 (false) = false; → false
        assert!(!evaluate_filter(&expr, &task_c));
    }

    #[test]
    fn test_eval_numeric_comparison() {
        let task = make_task("a", "active", "alice", Some(3));
        assert!(evaluate_filter(
            &parse_filter_expr("priority>2").unwrap(),
            &task
        ));
        assert!(!evaluate_filter(
            &parse_filter_expr("priority>3").unwrap(),
            &task
        ));
        assert!(evaluate_filter(
            &parse_filter_expr("priority>=3").unwrap(),
            &task
        ));
        assert!(evaluate_filter(
            &parse_filter_expr("priority<4").unwrap(),
            &task
        ));
        assert!(evaluate_filter(
            &parse_filter_expr("priority<=3").unwrap(),
            &task
        ));
        assert!(evaluate_filter(
            &parse_filter_expr("priority!=0").unwrap(),
            &task
        ));
    }

    #[test]
    fn test_validate_filter_expr_valid() {
        let expr = parse_filter_expr("status=active AND priority>0").unwrap();
        assert!(validate_filter_expr(&expr).is_ok());
    }

    #[test]
    fn test_validate_filter_expr_invalid_condition() {
        let expr = parse_filter_expr("nooperator AND status=active").unwrap();
        assert!(validate_filter_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_chained_and() {
        let task = make_task("a", "active", "alice", Some(0));
        let expr = parse_filter_expr("status=active AND owner=alice AND priority=0").unwrap();
        assert!(evaluate_filter(&expr, &task));
    }

    #[test]
    fn test_eval_chained_or() {
        let task = make_task("a", "active", "alice", Some(0));
        let expr = parse_filter_expr("status=done OR status=blocked OR status=active").unwrap();
        assert!(evaluate_filter(&expr, &task));
    }
}
