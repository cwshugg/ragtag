//! Generic tag creation from names, presets, overrides, and interactive edits.

use std::io::Write;
use std::path::Path;

use crate::config::{Config, TagPreset};
use crate::error::RagtagError;
use crate::input::prompt::{make_prompt, PromptSession};
use crate::models::{AttributeKind, AttributeValue, TagAttribute};
use crate::output::tag::{format_value, layout_tag, TagFormat};
use crate::parser::{
    parse_complete_attribute_value_with_lexeme, parse_complete_named_attribute_with_value_lexeme,
    parse_complete_tag, parse_complete_tag_with_value_lexemes, validate_complete_tag_name,
    validate_creatable_tag, validate_creatable_value, MAX_ATTRIBUTES_PER_TAG,
};

/// Canonical delimiter for interactive edits of delimiter-less values.
const DEFAULT_INTERACTIVE_DELIMITER: char = '"';

/// One semantic attribute paired with its validated source value expression.
#[derive(Debug, Clone)]
struct StyledAttribute {
    semantic: TagAttribute,
    raw_value: Option<String>,
}

/// Generic-create state that keeps lexical style local to this command.
#[derive(Debug, Clone)]
struct CreateTag {
    name: String,
    attributes: Vec<StyledAttribute>,
}

/// Runs generic tag creation and writes exactly one successful record.
pub fn run(
    matches: &clap::ArgMatches,
    config: &Config,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), RagtagError> {
    let mut tag = if let Some(name) = matches.get_one::<String>("name") {
        if name.starts_with('@') {
            return Err(invalid_tag_name());
        }
        let name = validate_complete_tag_name(name).map_err(|_| invalid_tag_name())?;
        CreateTag {
            name,
            attributes: Vec::new(),
        }
    } else {
        let Some(selector) = matches.get_one::<String>("preset") else {
            unreachable!("clap requires exactly one generic-create source");
        };
        resolve_preset(selector, &config.tags.presets)?
    };

    for (index, raw) in matches
        .get_many::<String>("attribute")
        .into_iter()
        .flatten()
        .enumerate()
    {
        let (semantic, raw_value) =
            parse_complete_named_attribute_with_value_lexeme(raw).map_err(|_| {
                RagtagError::Create(format!(
                    "invalid --attribute #{}: expected one complete named attribute (name=value)",
                    index + 1
                ))
            })?;
        validate_creatable_value(attribute_value(&semantic)).map_err(|_| {
            RagtagError::Create(format!(
                "invalid --attribute #{}: value cannot be represented safely",
                index + 1
            ))
        })?;
        upsert_attribute(
            &mut tag,
            StyledAttribute {
                semantic,
                raw_value: Some(raw_value),
            },
        )?;
    }

    if matches.get_flag("interactive") && !prompt_attributes(&mut tag, stderr)? {
        return Ok(());
    }
    let format = match matches.get_one::<String>("format").map(String::as_str) {
        Some("oneline") => TagFormat::Oneline,
        _ => TagFormat::Multiline,
    };
    let rendered = render_tag(&tag, format)?;
    writeln!(stdout, "{rendered}").map_err(RagtagError::Io)
}

/// Constructs the stable invalid-name diagnostic.
fn invalid_tag_name() -> RagtagError {
    RagtagError::Create(
        "invalid tag name: expected a parser-valid tag name without leading \"@\"".to_string(),
    )
}

/// Returns the value contained by either attribute shape.
fn attribute_value(attribute: &TagAttribute) -> &crate::models::AttributeValue {
    match &attribute.kind {
        AttributeKind::Named { value, .. } | AttributeKind::Positional { value } => value,
    }
}

/// Removes surrounding whitespace and at most one literal selector marker.
fn selector_body(value: &str) -> &str {
    let trimmed = value.trim();
    trimmed.strip_prefix('@').unwrap_or(trimmed)
}

/// Resolves a unique preset by lowercased nickname or contained tag name.
fn resolve_preset(selector: &str, presets: &[TagPreset]) -> Result<CreateTag, RagtagError> {
    let body = selector_body(selector);
    if body.is_empty() {
        return Err(RagtagError::Create(
            "preset selector must not be empty".into(),
        ));
    }
    let query = body.to_lowercase();
    let mut candidates = Vec::new();
    for (index, preset) in presets.iter().enumerate() {
        let (tag, raw_values) =
            parse_complete_tag_with_value_lexemes(&preset.value, Path::new("<config-tag-preset>"))
                .map_err(|_| RagtagError::Create(format!("invalid tags.presets[{index}].value")))?;
        validate_creatable_tag(&tag)
            .map_err(|_| RagtagError::Create(format!("invalid tags.presets[{index}].value")))?;
        let nickname = selector_body(&preset.nickname).to_lowercase();
        let tag_name = tag.name.to_lowercase();
        if query == nickname || query == tag_name {
            let attributes = tag
                .attributes
                .into_iter()
                .zip(raw_values)
                .map(|(semantic, raw_value)| StyledAttribute {
                    semantic,
                    raw_value: Some(raw_value),
                })
                .collect();
            candidates.push((
                index,
                CreateTag {
                    name: tag.name,
                    attributes,
                },
            ));
        }
    }
    match candidates.len() {
        0 => Err(RagtagError::Create(format!(
            "preset {selector:?} was not found by nickname or tag name"
        ))),
        1 => Ok(candidates.remove(0).1),
        _ => Err(RagtagError::Create(format!(
            "preset {selector:?} is ambiguous; matches {}",
            candidates
                .iter()
                .map(|(index, _)| format!("tags.presets[{index}]"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Applies replacement-and-deduplication semantics to one named attribute.
fn upsert_attribute(tag: &mut CreateTag, replacement: StyledAttribute) -> Result<(), RagtagError> {
    let AttributeKind::Named {
        name: replacement_name,
        ..
    } = &replacement.semantic.kind
    else {
        unreachable!("strict parser only returns named attributes");
    };
    let first = tag.attributes.iter().position(|attribute| {
        matches!(
            &attribute.semantic.kind,
            AttributeKind::Named { name, .. } if name == replacement_name
        )
    });
    if let Some(first) = first {
        tag.attributes[first] = replacement;
        let name = match &tag.attributes[first].semantic.kind {
            AttributeKind::Named { name, .. } => name.clone(),
            AttributeKind::Positional { .. } => unreachable!(),
        };
        let mut seen = false;
        tag.attributes
            .retain(|attribute| match &attribute.semantic.kind {
                AttributeKind::Named {
                    name: candidate, ..
                } if candidate == &name => {
                    let keep = !seen;
                    seen = true;
                    keep
                }
                _ => true,
            });
    } else {
        tag.attributes.push(replacement);
    }
    if tag.attributes.len() > MAX_ATTRIBUTES_PER_TAG {
        return Err(RagtagError::Create(format!(
            "tag exceeds the maximum of {MAX_ATTRIBUTES_PER_TAG} attributes"
        )));
    }
    Ok(())
}

/// Encodes one interactive response with numeric-literal passthrough.
///
/// Complete, safely representable numeric values retain their unquoted parser
/// lexeme. Other text uses the current quote delimiter, or double quotes when
/// the current value is delimiter-less. `None` denotes an unsafe numeric value.
fn encode_interactive_value(input: &str, current_raw_value: &str) -> Option<String> {
    if let Ok((value, raw_value)) = parse_complete_attribute_value_with_lexeme(input) {
        if matches!(
            value,
            AttributeValue::Integer { .. } | AttributeValue::Float(_)
        ) {
            validate_creatable_value(&value).ok()?;
            return Some(raw_value);
        }
    }
    let delimiter = match current_raw_value.chars().next() {
        Some(delimiter @ ('"' | '\'' | '`')) => delimiter,
        _ => DEFAULT_INTERACTIVE_DELIMITER,
    };
    let mut encoded = String::with_capacity(input.len() + 2);
    encoded.push(delimiter);
    for character in input.chars() {
        if character == '\\' || character == delimiter {
            encoded.push('\\');
        }
        encoded.push(character);
    }
    encoded.push(delimiter);
    Some(encoded)
}

/// Prompts once for every resulting attribute; blank input preserves the value.
fn prompt_attributes(tag: &mut CreateTag, stderr: &mut dyn Write) -> Result<bool, RagtagError> {
    let mut session = PromptSession::new()?;
    let mut positional_index = 0usize;
    for attribute in &mut tag.attributes {
        let (label, current) = match &attribute.semantic.kind {
            AttributeKind::Named { name, value } => (name.clone(), value),
            AttributeKind::Positional { value } => {
                positional_index += 1;
                (format!("Positional {positional_index}"), value)
            }
        };
        let current = match &attribute.raw_value {
            Some(raw_value) => raw_value.clone(),
            None => format_value(current)?,
        };
        let hint = format!("(current: {current}; Enter to keep)");
        let prompt = make_prompt(&label, Some(&hint), session.is_tty);
        let mut response_rejected = false;
        loop {
            let Some(input) = session.read_line(&prompt, stderr)? else {
                if session.cancelled {
                    writeln!(stderr, "Cancelled.").map_err(RagtagError::Io)?;
                    return Ok(false);
                }
                if response_rejected {
                    return Err(RagtagError::Io(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "unexpected end of input while waiting for a valid interactive attribute value",
                    )));
                }
                return Ok(true);
            };
            let input = input.strip_suffix('\n').unwrap_or(&input);
            let input = input.strip_suffix('\r').unwrap_or(input);
            if input.is_empty() {
                break;
            }
            let parsed = encode_interactive_value(input, &current).and_then(|encoded| {
                parse_complete_attribute_value_with_lexeme(&encoded)
                    .and_then(|(value, raw_value)| {
                        validate_creatable_value(&value).map(|()| (value, raw_value))
                    })
                    .ok()
            });
            match parsed {
                Some((value, raw_value)) => {
                    match &mut attribute.semantic.kind {
                        AttributeKind::Named {
                            value: destination, ..
                        }
                        | AttributeKind::Positional { value: destination } => {
                            *destination = value;
                        }
                    }
                    attribute.raw_value = Some(raw_value);
                    break;
                }
                None => {
                    session.write_error(stderr, "Expected safely representable attribute text.")?;
                    response_rejected = true;
                }
            }
        }
    }
    Ok(true)
}

/// Renders styled attributes and proves that their decoded semantics are unchanged.
fn render_tag(tag: &CreateTag, format: TagFormat) -> Result<String, RagtagError> {
    let attributes = tag
        .attributes
        .iter()
        .map(|attribute| {
            let value = attribute_value(&attribute.semantic);
            validate_creatable_value(value).map_err(|_| {
                RagtagError::Create("tag contains an unsafe attribute value".into())
            })?;
            let value = match &attribute.raw_value {
                Some(raw_value) => raw_value.clone(),
                None => format_value(value)?,
            };
            Ok(match &attribute.semantic.kind {
                AttributeKind::Named { name, .. } => format!("{name}={value}"),
                AttributeKind::Positional { .. } => value,
            })
        })
        .collect::<Result<Vec<_>, RagtagError>>()?;
    let rendered = layout_tag(&tag.name, &attributes, format);
    let reparsed = parse_complete_tag(&rendered, Path::new("<formatted-tag>")).map_err(|_| {
        RagtagError::InvalidInput("internal tag formatting validation failed".into())
    })?;
    let semantics_match = reparsed
        .attributes
        .iter()
        .eq(tag.attributes.iter().map(|attribute| &attribute.semantic));
    if reparsed.name != tag.name || !semantics_match {
        return Err(RagtagError::InvalidInput(
            "internal tag formatting changed tag semantics".into(),
        ));
    }
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructs a preset for lookup tests.
    fn preset(nickname: &str, value: &str) -> TagPreset {
        TagPreset {
            nickname: nickname.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn preset_resolution_deduplicates_keys_and_preserves_attributes() {
        let presets = [preset("issue", "@issue(positional, priority=1)")];
        let tag = resolve_preset("@ISSUE", &presets).unwrap();
        assert_eq!(tag.name, "issue");
        assert_eq!(tag.attributes.len(), 2);
    }

    #[test]
    fn preset_resolution_reports_every_ambiguity_in_config_order() {
        let presets = [
            preset("same", "@first"),
            preset("other", "@same"),
            preset("SAME", "@third"),
        ];
        let error = resolve_preset("same", &presets).unwrap_err().to_string();
        assert!(error.contains("matches tags.presets[0], tags.presets[1], tags.presets[2]"));
    }

    #[test]
    fn upsert_replaces_first_and_removes_later_duplicates() {
        let mut tag = resolve_preset(
            "test",
            &[preset(
                "test",
                "@tag(first=1, value='old', `middle`, value=duplicate)",
            )],
        )
        .unwrap();
        let (semantic, raw_value) =
            parse_complete_named_attribute_with_value_lexeme("value=`new`").unwrap();
        upsert_attribute(
            &mut tag,
            StyledAttribute {
                semantic,
                raw_value: Some(raw_value),
            },
        )
        .unwrap();
        assert_eq!(
            render_tag(&tag, TagFormat::Oneline).unwrap(),
            "@tag(first=1, value=`new`, `middle`)"
        );

        let (semantic, raw_value) =
            parse_complete_named_attribute_with_value_lexeme("appended='yes'").unwrap();
        upsert_attribute(
            &mut tag,
            StyledAttribute {
                semantic,
                raw_value: Some(raw_value),
            },
        )
        .unwrap();
        assert_eq!(
            render_tag(&tag, TagFormat::Oneline).unwrap(),
            "@tag(first=1, value=`new`, `middle`, appended='yes')"
        );
    }

    #[test]
    fn styled_render_rejects_raw_semantic_drift() {
        let (semantic, _) = parse_complete_named_attribute_with_value_lexeme("value=one").unwrap();
        let tag = CreateTag {
            name: "tag".to_string(),
            attributes: vec![StyledAttribute {
                semantic,
                raw_value: Some("two".to_string()),
            }],
        };
        let error = render_tag(&tag, TagFormat::Oneline).unwrap_err();
        assert!(error
            .to_string()
            .contains("internal tag formatting changed tag semantics"));
    }

    #[test]
    fn styled_render_falls_back_to_canonical_value_formatting() {
        let tag = parse_complete_tag("@tag(value='fallback')", Path::new("<test>")).unwrap();
        let tag = CreateTag {
            name: tag.name,
            attributes: tag
                .attributes
                .into_iter()
                .map(|semantic| StyledAttribute {
                    semantic,
                    raw_value: None,
                })
                .collect(),
        };
        assert_eq!(
            render_tag(&tag, TagFormat::Oneline).unwrap(),
            r#"@tag(value="fallback")"#
        );
    }

    #[test]
    fn interactive_text_encoding_preserves_or_defaults_delimiters() {
        assert_eq!(
            encode_interactive_value(r#"a"b\c`d"#, r#""current""#),
            Some(r#""a\"b\\c`d""#.to_string())
        );
        assert_eq!(
            encode_interactive_value("a`b\\c\"d", "`current`"),
            Some("`a\\`b\\\\c\"d`".to_string())
        );
        assert_eq!(
            encode_interactive_value(" 42 ", "0X2A"),
            Some("42".to_string())
        );
        assert_eq!(
            encode_interactive_value("1.0e3", "`current`"),
            Some("1.0e3".to_string())
        );
        assert_eq!(
            encode_interactive_value("1e3", "`current`"),
            Some("`1e3`".to_string())
        );
        assert_eq!(encode_interactive_value("1.0e999", "\"current\""), None);
    }

    #[test]
    fn upsert_enforces_aggregate_limit_without_penalizing_replacement() {
        let attributes = (0..MAX_ATTRIBUTES_PER_TAG)
            .map(|index| format!("a{index}={index}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut tag =
            resolve_preset("test", &[preset("test", &format!("@tag({attributes})"))]).unwrap();
        let (semantic, raw_value) =
            parse_complete_named_attribute_with_value_lexeme("a0=replaced").unwrap();
        upsert_attribute(
            &mut tag,
            StyledAttribute {
                semantic,
                raw_value: Some(raw_value),
            },
        )
        .unwrap();
        assert_eq!(tag.attributes.len(), MAX_ATTRIBUTES_PER_TAG);
        let (semantic, raw_value) =
            parse_complete_named_attribute_with_value_lexeme("new_name=value").unwrap();
        assert!(upsert_attribute(
            &mut tag,
            StyledAttribute {
                semantic,
                raw_value: Some(raw_value),
            }
        )
        .is_err());
    }
}
