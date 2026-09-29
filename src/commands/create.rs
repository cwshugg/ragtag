//! Generic tag creation from names, presets, overrides, and interactive edits.

use std::io::Write;
use std::path::Path;

use crate::config::schema::lowercase_tag_lookup_key;
use crate::config::{Config, TagPreset};
use crate::error::RagtagError;
use crate::input::prompt::{make_prompt, PromptSession};
use crate::models::{AttributeKind, Tag, TagAttribute};
use crate::output::tag::{format_tag, format_value, TagFormat};
use crate::parser::{
    parse_complete_attribute_value, parse_complete_named_attribute, parse_complete_tag,
    validate_complete_tag_name, validate_creatable_tag, validate_creatable_value,
    MAX_ATTRIBUTES_PER_TAG,
};

/// Synthetic source used for values created outside a real file.
const CREATE_SOURCE: &str = "<create>";

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
        parse_complete_tag(&format!("@{name}"), Path::new(CREATE_SOURCE))
            .map_err(|_| invalid_tag_name())?
    } else {
        let Some(selector) = matches.get_one::<String>("preset") else {
            return Err(RagtagError::Create(
                "exactly one of --name or --preset is required".into(),
            ));
        };
        resolve_preset(selector, &config.tags.presets)?
    };

    for (index, raw) in matches
        .get_many::<String>("attribute")
        .into_iter()
        .flatten()
        .enumerate()
    {
        let attribute = parse_complete_named_attribute(raw).map_err(|_| {
            RagtagError::Create(format!(
                "invalid --attribute #{}: expected one complete named attribute (name=value)",
                index + 1
            ))
        })?;
        let value = attribute_value(&attribute);
        validate_creatable_value(value).map_err(|_| {
            RagtagError::Create(format!(
                "invalid --attribute #{}: value cannot be represented safely",
                index + 1
            ))
        })?;
        upsert_attribute(&mut tag, attribute)?;
    }

    if matches.get_flag("interactive") && !prompt_attributes(&mut tag, stderr)? {
        return Ok(());
    }
    validate_creatable_tag(&tag)
        .map_err(|_| RagtagError::Create("tag contains an unsafe attribute value".into()))?;
    let format = match matches.get_one::<String>("format").map(String::as_str) {
        Some("oneline") => TagFormat::Oneline,
        _ => TagFormat::Multiline,
    };
    let rendered = format_tag(&tag, format)?;
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

/// Produces a best-effort lowercase preset lookup key.
fn lookup_key(value: &str) -> String {
    lowercase_tag_lookup_key(value)
}

/// Removes surrounding whitespace and at most one literal selector marker.
fn selector_body(value: &str) -> &str {
    let trimmed = value.trim();
    trimmed.strip_prefix('@').unwrap_or(trimmed)
}

/// Resolves a unique preset by lowercased nickname or contained tag name.
fn resolve_preset(selector: &str, presets: &[TagPreset]) -> Result<Tag, RagtagError> {
    let body = selector_body(selector);
    if body.is_empty() {
        return Err(RagtagError::Create(
            "preset selector must not be empty".into(),
        ));
    }
    let query = lookup_key(body);
    let mut candidates = Vec::new();
    for (index, preset) in presets.iter().enumerate() {
        let tag = parse_complete_tag(&preset.value, Path::new("<config-tag-preset>"))
            .map_err(|_| RagtagError::Create(format!("invalid tags.presets[{index}].value")))?;
        let nickname = lookup_key(selector_body(&preset.nickname));
        let tag_name = lookup_key(&tag.name);
        if query == nickname || query == tag_name {
            candidates.push((index, tag));
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
fn upsert_attribute(tag: &mut Tag, replacement: TagAttribute) -> Result<(), RagtagError> {
    let AttributeKind::Named {
        name: replacement_name,
        ..
    } = &replacement.kind
    else {
        unreachable!("strict parser only returns named attributes");
    };
    let first = tag.attributes.iter().position(|attribute| {
        matches!(
            &attribute.kind,
            AttributeKind::Named { name, .. } if name == replacement_name
        )
    });
    if let Some(first) = first {
        tag.attributes[first] = replacement;
        let name = match &tag.attributes[first].kind {
            AttributeKind::Named { name, .. } => name.clone(),
            AttributeKind::Positional { .. } => unreachable!(),
        };
        let mut seen = false;
        tag.attributes.retain(|attribute| match &attribute.kind {
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

/// Prompts once for every resulting attribute; blank input preserves the value.
fn prompt_attributes(tag: &mut Tag, stderr: &mut dyn Write) -> Result<bool, RagtagError> {
    let mut session = PromptSession::new()?;
    let mut positional_index = 0usize;
    for attribute in &mut tag.attributes {
        let (label, current) = match &attribute.kind {
            AttributeKind::Named { name, value } => (name.clone(), value),
            AttributeKind::Positional { value } => {
                positional_index += 1;
                (format!("Positional {positional_index}"), value)
            }
        };
        let hint = format!("(current: {}; Enter to keep)", format_value(current)?);
        let prompt = make_prompt(&label, Some(&hint), session.is_tty);
        loop {
            let Some(input) = session.read_line(&prompt, stderr)? else {
                if session.cancelled {
                    writeln!(stderr, "Cancelled.").map_err(RagtagError::Io)?;
                    return Ok(false);
                }
                return Ok(true);
            };
            if input.trim().is_empty() {
                break;
            }
            match parse_complete_attribute_value(&input)
                .and_then(|value| validate_creatable_value(&value).map(|()| value))
            {
                Ok(value) => {
                    match &mut attribute.kind {
                        AttributeKind::Named {
                            value: destination, ..
                        }
                        | AttributeKind::Positional { value: destination } => {
                            *destination = value;
                        }
                    }
                    break;
                }
                Err(_) => session.write_error(
                    stderr,
                    "Expected one complete, safely representable attribute value.",
                )?,
            }
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive_for_ordinary_english_nicknames() {
        assert_eq!(lookup_key("Bug Report"), lookup_key("BUG REPORT"));
        assert_eq!(lookup_key("MeetingNotes"), "meetingnotes");
    }

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
        let mut tag = parse_complete_tag(
            "@tag(first=1, value=old, middle, value=duplicate)",
            Path::new("<test>"),
        )
        .unwrap();
        let replacement = parse_complete_named_attribute("value=new").unwrap();
        upsert_attribute(&mut tag, replacement).unwrap();
        assert_eq!(
            format_tag(&tag, TagFormat::Oneline).unwrap(),
            r#"@tag(first=1, value="new", "middle")"#
        );
    }

    #[test]
    fn upsert_enforces_aggregate_limit_without_penalizing_replacement() {
        let attributes = (0..MAX_ATTRIBUTES_PER_TAG)
            .map(|index| format!("a{index}={index}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut tag =
            parse_complete_tag(&format!("@tag({attributes})"), Path::new("<test>")).unwrap();
        upsert_attribute(
            &mut tag,
            parse_complete_named_attribute("a0=replaced").unwrap(),
        )
        .unwrap();
        assert_eq!(tag.attributes.len(), MAX_ATTRIBUTES_PER_TAG);
        assert!(upsert_attribute(
            &mut tag,
            parse_complete_named_attribute("new_name=value").unwrap()
        )
        .is_err());
    }
}
