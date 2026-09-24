//! Deterministic D2 v0.9.0 source writer.

use std::collections::BTreeMap;

use crate::error::RagtagError;

use super::super::document::{Document, Element, ElementRole, StatusRole};

/// Serializes one complete neutral document before any sink is opened.
pub(crate) fn serialize(document: &Document, maximum_bytes: usize) -> Result<Vec<u8>, RagtagError> {
    let elements = document
        .elements
        .iter()
        .map(|element| (element.id.as_str(), element))
        .collect::<BTreeMap<_, _>>();
    let mut children = BTreeMap::<&str, Vec<&Element>>::new();
    for element in document
        .elements
        .iter()
        .filter(|element| element.parent.is_some())
    {
        children
            .entry(element.parent.as_deref().expect("filtered"))
            .or_default()
            .push(element);
    }
    let mut output = String::new();
    push(&mut output, "direction: ", maximum_bytes)?;
    push(&mut output, document.direction.d2(), maximum_bytes)?;
    push(&mut output, "\n", maximum_bytes)?;
    enum Event<'a> {
        Element(&'a Element, usize),
        Close(usize),
    }
    let mut events = elements
        .values()
        .filter(|element| element.parent.is_none())
        .rev()
        .map(|element| Event::Element(element, 0))
        .collect::<Vec<_>>();
    while let Some(event) = events.pop() {
        match event {
            Event::Close(depth) => {
                push(
                    &mut output,
                    &format!("{}}}\n", "  ".repeat(depth)),
                    maximum_bytes,
                )?;
            }
            Event::Element(element, depth) => {
                let element_children = children
                    .get(element.id.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let indent = "  ".repeat(depth);
                let element_key = key(&element.id);
                let label = format!(
                    "{}\\n{}",
                    quote(&element.label.first),
                    quote(&element.label.second)
                );
                if element.role == ElementRole::Bucket && !element_children.is_empty() {
                    push(
                        &mut output,
                        &format!("{indent}{element_key}: {{\n"),
                        maximum_bytes,
                    )?;
                    push(
                        &mut output,
                        &format!("{indent}  label: \"{label}\"\n"),
                        maximum_bytes,
                    )?;
                    style(&mut output, element, depth + 1, None, maximum_bytes)?;
                    events.push(Event::Close(depth));
                    events.extend(
                        element_children
                            .iter()
                            .rev()
                            .map(|child| Event::Element(child, depth + 1)),
                    );
                } else {
                    push(
                        &mut output,
                        &format!("{indent}{element_key}: \"{label}\"\n"),
                        maximum_bytes,
                    )?;
                    style(
                        &mut output,
                        element,
                        depth,
                        Some(&element_key),
                        maximum_bytes,
                    )?;
                }
            }
        }
    }
    for edge in &document.edges {
        push(
            &mut output,
            &format!("{} -> {}\n", key(&edge.from), key(&edge.to)),
            maximum_bytes,
        )?;
    }
    Ok(output.into_bytes())
}

fn style(
    output: &mut String,
    element: &Element,
    depth: usize,
    element_key: Option<&str>,
    maximum: usize,
) -> Result<(), RagtagError> {
    let indent = "  ".repeat(depth);
    let prefix = element_key.map_or("style".to_string(), |key| format!("{key}.style"));
    let (fill, stroke) = match element.status {
        StatusRole::Done => ("#d3f9d8", "#2b8a3e"),
        StatusRole::Active => ("#fff3bf", "#e67700"),
        StatusRole::Blocked => ("#ffe3e3", "#c92a2a"),
        StatusRole::Abandoned => ("#ffe8cc", "#d9480f"),
        StatusRole::Inactive => ("#e9ecef", "#868e96"),
    };
    push(
        output,
        &format!("{indent}{prefix}.fill: \"{fill}\"\n"),
        maximum,
    )?;
    push(
        output,
        &format!("{indent}{prefix}.stroke: \"{stroke}\"\n"),
        maximum,
    )?;
    let stroke_width = match element.priority {
        Some(0) => 4,
        Some(1) => 3,
        Some(2) => 2,
        _ => 1,
    };
    push(
        output,
        &format!("{indent}{prefix}.stroke-width: {stroke_width}\n"),
        maximum,
    )?;
    push(
        output,
        &format!("{indent}{prefix}.border-radius: 6\n"),
        maximum,
    )?;
    if element.context {
        push(
            output,
            &format!("{indent}{prefix}.opacity: 0.55\n"),
            maximum,
        )?;
        push(
            output,
            &format!("{indent}{prefix}.stroke-dash: 4\n"),
            maximum,
        )?;
    }
    Ok(())
}

fn key(value: &str) -> String {
    let mut output = String::from("n");
    for byte in value.as_bytes() {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn quote(value: &str) -> String {
    let mut output = String::new();
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '$' if characters.peek() == Some(&'{') => output.push_str("\\$"),
            _ => output.push(character),
        }
    }
    output
}

fn push(output: &mut String, value: &str, maximum: usize) -> Result<(), RagtagError> {
    if output.len().saturating_add(value.len()) > maximum {
        return Err(RagtagError::Diagram(
            "serialized D2 exceeds the configured byte limit".to_string(),
        ));
    }
    output.push_str(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::document::{Direction, Edge, Label, StatusRole};

    #[test]
    fn hostile_values_are_quoted_and_keys_are_injective() {
        let document = Document {
            direction: Direction::Down,
            elements: vec![Element {
                id: "a.b ${id} \"".to_string(),
                parent: None,
                label: Label {
                    first: "\" ${x} @import link:".to_string(),
                    second: "owner ${y} class: evil".to_string(),
                },
                role: ElementRole::Task,
                status: StatusRole::Blocked,
                context: true,
                priority: Some(0),
            }],
            edges: Vec::<Edge>::new(),
        };
        let output = String::from_utf8(serialize(&document, 4096).unwrap()).unwrap();
        assert!(output.contains("n612e6220247b69647d2022"));
        assert!(output.contains("\\\" \\${x}"));
        assert!(output.contains("\\${y}"));
        assert!(output.contains(".style.fill: \"#ffe3e3\""));
        assert!(output.contains(".style.stroke-width: 4"));
        assert!(output.contains(".style.opacity: 0.55"));
        assert_eq!(output.matches("\\n").count(), 1);
        assert!(output.ends_with('\n'));
    }
}
