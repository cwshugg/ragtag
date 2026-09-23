//! Single-pass environment interpolation for parsed configuration strings.

use serde_yml::Value;
use std::collections::HashSet;

/// Values that originated from environment interpolation.
#[derive(Clone, Default)]
pub(crate) struct InterpolationProvenance {
    sensitive_values: HashSet<String>,
}

impl std::fmt::Debug for InterpolationProvenance {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InterpolationProvenance")
            .field("derived_value_count", &self.sensitive_values.len())
            .finish()
    }
}

impl InterpolationProvenance {
    /// Returns whether any environment reference was expanded.
    pub(crate) fn is_empty(&self) -> bool {
        self.sensitive_values.is_empty()
    }

    /// Returns whether one complete value originated from interpolation.
    pub(crate) fn contains(&self, value: &str) -> bool {
        self.sensitive_values.contains(value)
    }

    /// Records the complete derived string used by typed configuration.
    fn record(&mut self, output: &str) {
        self.sensitive_values.insert(output.to_string());
    }
}

/// Traversal context used to defer only top-level alias argument templates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueContext {
    Root,
    Normal,
    AliasCollection,
    AliasDefinition,
    AliasArguments,
}

/// Returns whether a byte can begin a supported environment variable name.
fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// Returns whether a byte can continue a supported environment variable name.
fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

/// Expands one string exactly once using a caller-provided environment lookup.
///
/// Supported references are `$NAME` and `${NAME}`. Undefined names become an
/// empty string, `$$` becomes one literal dollar, and malformed references are
/// preserved byte-for-byte.
#[cfg(test)]
fn interpolate_string_with<F>(input: &str, lookup: &mut F) -> String
where
    F: FnMut(&str) -> Option<String>,
{
    interpolate_string_with_status(input, lookup).0
}

/// Expands one string and reports whether it contained an environment reference.
pub(crate) fn interpolate_string_with_status<F>(input: &str, lookup: &mut F) -> (String, bool)
where
    F: FnMut(&str) -> Option<String>,
{
    interpolate_string_internal(input, lookup)
}

/// Performs one interpolation pass and tracks whether a reference was expanded.
fn interpolate_string_internal<F>(input: &str, lookup: &mut F) -> (String, bool)
where
    F: FnMut(&str) -> Option<String>,
{
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut environment_derived = false;
    let mut cursor = 0;
    let mut literal_start = 0;

    while cursor < bytes.len() {
        if bytes[cursor] != b'$' {
            cursor += 1;
            continue;
        }

        output.push_str(&input[literal_start..cursor]);
        let next = cursor + 1;
        if next >= bytes.len() {
            output.push('$');
            cursor = next;
            literal_start = cursor;
            continue;
        }

        if bytes[next] == b'$' {
            output.push('$');
            cursor += 2;
            literal_start = cursor;
            continue;
        }

        if bytes[next] == b'{' {
            let name_start = next + 1;
            let Some(relative_end) = bytes[name_start..].iter().position(|byte| *byte == b'}')
            else {
                output.push_str(&input[cursor..]);
                return (output, environment_derived);
            };
            let name_end = name_start + relative_end;
            let name = &bytes[name_start..name_end];
            if !name.is_empty()
                && is_name_start(name[0])
                && name.iter().copied().all(is_name_continue)
            {
                let value = lookup(&input[name_start..name_end]).unwrap_or_default();
                output.push_str(&value);
                environment_derived = true;
                cursor = name_end + 1;
                literal_start = cursor;
                continue;
            }

            output.push_str(&input[cursor..=name_end]);
            cursor = name_end + 1;
            literal_start = cursor;
            continue;
        }

        if is_name_start(bytes[next]) {
            let mut name_end = next + 1;
            while name_end < bytes.len() && is_name_continue(bytes[name_end]) {
                name_end += 1;
            }
            let value = lookup(&input[next..name_end]).unwrap_or_default();
            output.push_str(&value);
            environment_derived = true;
            cursor = name_end;
            literal_start = cursor;
            continue;
        }

        output.push('$');
        cursor += 1;
        literal_start = cursor;
    }

    output.push_str(&input[literal_start..]);
    (output, environment_derived)
}

/// Recursively interpolates parsed YAML values with an injected lookup.
pub(crate) fn interpolate_config_value_with<F>(
    value: &mut Value,
    lookup: &mut F,
) -> InterpolationProvenance
where
    F: FnMut(&str) -> Option<String>,
{
    let mut provenance = InterpolationProvenance::default();
    interpolate_value(value, ValueContext::Root, lookup, &mut provenance);
    provenance
}

/// Traverses values without mutating mapping keys, tags, or alias templates.
fn interpolate_value<F>(
    value: &mut Value,
    context: ValueContext,
    lookup: &mut F,
    provenance: &mut InterpolationProvenance,
) where
    F: FnMut(&str) -> Option<String>,
{
    if context == ValueContext::AliasArguments {
        return;
    }

    match value {
        Value::String(text) => {
            let (output, environment_derived) = interpolate_string_internal(text, lookup);
            if environment_derived {
                provenance.record(&output);
            }
            *text = output;
        }
        Value::Sequence(sequence) => {
            let child_context = if context == ValueContext::AliasCollection {
                ValueContext::AliasDefinition
            } else {
                ValueContext::Normal
            };
            for child in sequence {
                interpolate_value(child, child_context, lookup, provenance);
            }
        }
        Value::Mapping(mapping) => {
            for (key, child) in mapping {
                let child_context = match (context, key) {
                    (ValueContext::Root, Value::String(name)) if name == "aliases" => {
                        ValueContext::AliasCollection
                    }
                    (ValueContext::AliasDefinition, Value::String(name)) if name == "arguments" => {
                        ValueContext::AliasArguments
                    }
                    _ => ValueContext::Normal,
                };
                interpolate_value(child, child_context, lookup, provenance);
            }
        }
        Value::Tagged(tagged) => {
            interpolate_value(&mut tagged.value, context, lookup, provenance);
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Interpolates with an immutable test map and no process environment.
    fn expand(input: &str, values: &[(&str, &str)]) -> String {
        let values = values.iter().copied().collect::<HashMap<_, _>>();
        interpolate_string_with(input, &mut |name| {
            values.get(name).map(|value| (*value).to_string())
        })
    }

    #[test]
    fn string_interpolation_covers_supported_syntax_and_boundaries() {
        let values = [
            ("A", "one"),
            ("B_2", "two"),
            ("EMPTY", ""),
            ("FOO", "short"),
            ("FOOBAR", "long"),
        ];
        assert_eq!(expand("$A/${B_2}/$A$B_2", &values), "one/two/onetwo");
        assert_eq!(expand("$FOOBAR:$FOO", &values), "long:short");
        assert_eq!(expand("[$MISSING][${EMPTY}]", &values), "[][]");
        assert_eq!(expand("π-$A-雪", &values), "π-one-雪");
    }

    #[test]
    fn dollar_escaping_and_single_pass_are_deterministic() {
        let values = [("VALUE", "$OTHER"), ("OTHER", "expanded")];
        assert_eq!(expand("$$VALUE", &values), "$VALUE");
        assert_eq!(expand("$$$VALUE", &values), "$$OTHER");
        assert_eq!(expand("$VALUE", &values), "$OTHER");
    }

    #[test]
    fn malformed_references_are_preserved_literally() {
        for input in [
            "$",
            "${",
            "${}",
            "${9BAD}",
            "${BAD-NAME}",
            "${MISSING",
            "$9",
            "$-",
        ] {
            assert_eq!(expand(input, &[]), input);
        }
    }

    #[test]
    fn malformed_braced_references_are_atomic_and_do_not_expand_nested_dollars() {
        let values = [("GOOD", "expanded"), ("B", "nested"), ("AFTER", "after")];
        for (input, expected) in [
            ("${BAD-$GOOD}", "${BAD-$GOOD}"),
            ("${A${B}}", "${A${B}}"),
            ("${MISSING$GOOD", "${MISSING$GOOD"),
            ("before-${BAD-$GOOD}-$AFTER", "before-${BAD-$GOOD}-after"),
            ("${A${B}}$AFTER", "${A${B}}after"),
            ("${MISSING$GOOD$AFTER", "${MISSING$GOOD$AFTER"),
        ] {
            assert_eq!(expand(input, &values), expected);
        }
    }

    #[test]
    fn provenance_tracks_complete_environment_derived_values() {
        let mut value = Value::String("prefix-$SECRET".to_string());
        let provenance = interpolate_config_value_with(&mut value, &mut |name| {
            (name == "SECRET").then(|| "sentinel-value".to_string())
        });

        assert_eq!(value, Value::String("prefix-sentinel-value".to_string()));
        assert!(provenance.contains("prefix-sentinel-value"));
        assert!(!provenance.contains("sentinel-value"));
    }

    #[test]
    fn yaml_traversal_expands_values_but_preserves_structure_and_alias_arguments() {
        let yaml = r#"
"$KEY": "$VALUE"
nested:
  - "$VALUE"
  - inner: "${EMPTY}"
tagged: !example "$VALUE"
injectable: "$STRUCTURE"
aliases:
  - name: "$ALIAS"
    arguments: 'query "$RUNTIME"'
    description: "$DESCRIPTION"
custom:
  arguments: "$VALUE"
"#;
        let mut value: Value = serde_yml::from_str(yaml).unwrap();
        let values = HashMap::from([
            ("KEY", "changed-key"),
            ("VALUE", "resolved"),
            ("EMPTY", ""),
            ("STRUCTURE", "[injected, sequence]"),
            ("ALIAS", "dynamic"),
            ("RUNTIME", "deferred"),
            ("DESCRIPTION", "metadata"),
        ]);
        interpolate_config_value_with(&mut value, &mut |name| {
            values.get(name).map(|value| (*value).to_string())
        });

        let root = value.as_mapping().unwrap();
        assert_eq!(
            root.get(Value::String("$KEY".to_string())),
            Some(&Value::String("resolved".to_string()))
        );
        assert!(!root.contains_key(Value::String("changed-key".to_string())));
        assert_eq!(
            root.get(Value::String("injectable".to_string())),
            Some(&Value::String("[injected, sequence]".to_string()))
        );
        let nested = root
            .get(Value::String("nested".to_string()))
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(nested[0], Value::String("resolved".to_string()));
        assert_eq!(
            nested[1]
                .as_mapping()
                .unwrap()
                .get(Value::String("inner".to_string())),
            Some(&Value::String(String::new()))
        );
        match root.get(Value::String("tagged".to_string())).unwrap() {
            Value::Tagged(tagged) => {
                assert_eq!(tagged.tag.to_string(), "!example");
                assert_eq!(tagged.value, Value::String("resolved".to_string()));
            }
            other => panic!("expected tagged YAML value, got {other:?}"),
        }

        let aliases = root
            .get(Value::String("aliases".to_string()))
            .unwrap()
            .as_sequence()
            .unwrap();
        let alias = aliases[0].as_mapping().unwrap();
        assert_eq!(
            alias.get(Value::String("name".to_string())),
            Some(&Value::String("dynamic".to_string()))
        );
        assert_eq!(
            alias.get(Value::String("arguments".to_string())),
            Some(&Value::String("query \"$RUNTIME\"".to_string()))
        );
        assert_eq!(
            alias.get(Value::String("description".to_string())),
            Some(&Value::String("metadata".to_string()))
        );

        let custom = root
            .get(Value::String("custom".to_string()))
            .unwrap()
            .as_mapping()
            .unwrap();
        assert_eq!(
            custom.get(Value::String("arguments".to_string())),
            Some(&Value::String("resolved".to_string()))
        );
    }
}
