//! Pure alias indexing, raw-argument scanning, resolution, and composition.
//!
//! Original arguments remain OS-native through scanning, expansion, and final
//! argv assembly. The real clap command tree owns terminal value parsing, so
//! individual command arguments may still impose Unicode requirements.

use crate::config::Alias;
use crate::error::RagtagError;
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};

use super::{classify_root_token, RootToken};

/// Maximum number of alias definitions in one expansion chain.
pub const MAX_ALIAS_EXPANSION_DEPTH: usize = 32;

/// Maximum number of expanded tokens, excluding argv[0] and leading globals.
pub const MAX_EXPANDED_ARGUMENTS: usize = 4096;

/// A validated, ordered lookup over alias definitions and all peer names.
#[derive(Debug)]
pub struct AliasIndex<'a> {
    definitions: &'a [Alias],
    by_name: HashMap<&'a str, usize>,
    ordered_names: Vec<(&'a str, usize)>,
}

impl<'a> AliasIndex<'a> {
    /// Builds an index after `Config::validate_aliases` has succeeded.
    pub fn new(definitions: &'a [Alias]) -> Self {
        let mut by_name = HashMap::new();
        let mut ordered_names = Vec::new();
        for (definition_index, alias) in definitions.iter().enumerate() {
            debug_assert!(!alias.names.is_empty(), "validated alias has a name");
            for name in &alias.names {
                let previous = by_name.insert(name.as_str(), definition_index);
                debug_assert!(previous.is_none(), "validated aliases have unique names");
                ordered_names.push((name.as_str(), definition_index));
            }
        }
        Self {
            definitions,
            by_name,
            ordered_names,
        }
    }

    /// Returns the stable definition identity associated with an exact name.
    pub fn definition_for_name(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    /// Returns the canonical first configured name for one definition.
    fn canonical_name(&self, definition_index: usize) -> &str {
        self.definitions[definition_index]
            .names
            .first()
            .map_or("", String::as_str)
    }
}

/// Result of scanning the original argv for a possible outer command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OuterScan {
    /// A candidate command was found at the original token index.
    Command(usize),
    /// Alias classification is bypassed and the original argv goes to clap.
    Terminal,
}

/// Scans only recognized leading root constructs without normalizing tokens.
pub fn scan_outer_command(args: &[OsString]) -> OuterScan {
    let mut index = 1;
    while index < args.len() {
        let token = args[index].as_os_str();
        match classify_root_token(token) {
            RootToken::Separator | RootToken::Option => {
                return OuterScan::Terminal;
            }
            RootToken::NoColor | RootToken::ConfigEquals(_) => index += 1,
            RootToken::ConfigSplit => {
                if index + 1 >= args.len()
                    || classify_root_token(args[index + 1].as_os_str()) == RootToken::Separator
                {
                    return OuterScan::Terminal;
                }
                index += 2;
            }
            RootToken::Command => return OuterScan::Command(index),
        }
    }
    OuterScan::Terminal
}

/// Resolution of a prospective outer command against real and alias names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OuterResolution {
    /// The token belongs to clap's real command tree.
    Real,
    /// The token selected an alias definition and canonical configured name.
    Alias {
        /// Stable alias definition identity.
        definition_index: usize,
        /// Exact or uniquely inferred configured spelling.
        selected_name: String,
    },
    /// No combined-name candidate exists; clap receives the original argv.
    None,
}

/// Resolves an outer token with real-name precedence and deterministic ordering.
pub fn resolve_outer_command(
    token: &OsStr,
    real_names: &[String],
    aliases: &AliasIndex<'_>,
) -> Result<OuterResolution, RagtagError> {
    let Some(token) = token.to_str().filter(|token| !token.is_empty()) else {
        return Ok(OuterResolution::None);
    };

    if real_names.iter().any(|name| name == token) {
        return Ok(OuterResolution::Real);
    }
    if let Some(definition_index) = aliases.definition_for_name(token) {
        return Ok(OuterResolution::Alias {
            definition_index,
            selected_name: token.to_string(),
        });
    }

    let mut real_candidates = Vec::new();
    for name in real_names {
        if name.starts_with(token) {
            real_candidates.push(name.clone());
        }
    }

    let mut alias_candidates = Vec::new();
    let mut seen_definitions = HashSet::new();
    for &(name, definition_index) in &aliases.ordered_names {
        if name.starts_with(token) && seen_definitions.insert(definition_index) {
            alias_candidates.push((definition_index, name));
        }
    }

    if alias_candidates.is_empty() {
        return if real_candidates.is_empty() {
            Ok(OuterResolution::None)
        } else {
            // clap remains authoritative for both unique and ambiguous
            // prefixes that involve only real commands.
            Ok(OuterResolution::Real)
        };
    }

    if real_candidates.is_empty() && alias_candidates.len() == 1 {
        let definition_index = alias_candidates[0].0;
        return Ok(OuterResolution::Alias {
            definition_index,
            selected_name: aliases.canonical_name(definition_index).to_string(),
        });
    }

    let candidates = real_candidates
        .into_iter()
        .chain(
            alias_candidates
                .into_iter()
                .map(|(_, matching_name)| matching_name.to_string()),
        )
        .collect();
    Err(RagtagError::AliasOuterAmbiguous {
        token: token.to_string(),
        candidates,
    })
}

/// Tokens produced by recursive alias composition before terminal argv assembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    /// Expanded command tokens, excluding argv[0] and leading root globals.
    pub working: Vec<OsString>,
    /// Number of leading working tokens supplied by alias definitions.
    configured_len: usize,
}

impl Expansion {
    /// Returns config paths introduced by trusted alias definitions.
    ///
    /// These selectors remain terminal clap syntax and never reload startup
    /// configuration. The list is used only to distinguish them from original
    /// post-command `--config` occurrences during reconciliation.
    pub fn alias_defined_config_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        let mut index = 0;
        while index < self.configured_len {
            match classify_root_token(self.working[index].as_os_str()) {
                RootToken::Separator => break,
                RootToken::ConfigEquals(value) => {
                    paths.push(std::path::PathBuf::from(value));
                    index += 1;
                }
                RootToken::ConfigSplit => {
                    let Some(value) = self.working.get(index + 1) else {
                        break;
                    };
                    if classify_root_token(value.as_os_str()) == RootToken::Separator {
                        break;
                    }
                    paths.push(std::path::PathBuf::from(value));
                    index += 2;
                }
                RootToken::NoColor | RootToken::Option | RootToken::Command => index += 1,
            }
        }
        paths
    }
}

/// Computes a replacement projection and enforces the expanded-token bound.
fn checked_projected_count(
    current: usize,
    removed: usize,
    added: usize,
    chain: &[String],
) -> Result<usize, RagtagError> {
    let projected = current
        .checked_sub(removed)
        .and_then(|count| count.checked_add(added))
        .ok_or_else(|| RagtagError::AliasExpansionArgumentsExceeded {
            limit: MAX_EXPANDED_ARGUMENTS,
            count: usize::MAX,
            chain: chain.to_vec(),
        })?;
    if projected > MAX_EXPANDED_ARGUMENTS {
        return Err(RagtagError::AliasExpansionArgumentsExceeded {
            limit: MAX_EXPANDED_ARGUMENTS,
            count: projected,
            chain: chain.to_vec(),
        });
    }
    Ok(projected)
}

/// Recursively replaces token zero on exact alias-name references.
pub fn expand_alias(
    aliases: &AliasIndex<'_>,
    definition_index: usize,
    selected_name: &str,
    original_suffix: &[OsString],
    real_names: &[String],
) -> Result<Expansion, RagtagError> {
    let outer = &aliases.definitions[definition_index];
    let mut chain = vec![selected_name.to_string()];
    let initial_count =
        checked_projected_count(outer.arguments.len(), 0, original_suffix.len(), &chain)?;
    let mut working = Vec::with_capacity(initial_count);
    working.extend(outer.arguments.iter().map(OsString::from));
    working.extend(original_suffix.iter().cloned());

    let mut active_definitions = HashSet::new();
    active_definitions.insert(definition_index);

    while let Some(reference_name) = working
        .first()
        .and_then(|token| token.to_str())
        .map(str::to_string)
    {
        let Some(next_index) = aliases.definition_for_name(&reference_name) else {
            break;
        };

        let mut next_chain = chain.clone();
        next_chain.push(reference_name);
        if active_definitions.contains(&next_index) {
            return Err(RagtagError::AliasCycle { chain: next_chain });
        }
        if chain.len() >= MAX_ALIAS_EXPANSION_DEPTH {
            return Err(RagtagError::AliasExpansionDepthExceeded {
                limit: MAX_ALIAS_EXPANSION_DEPTH,
                chain: next_chain,
            });
        }

        let replacement = &aliases.definitions[next_index].arguments;
        let projected = checked_projected_count(working.len(), 1, replacement.len(), &next_chain)?;
        let mut next = Vec::with_capacity(projected);
        next.extend(replacement.iter().map(OsString::from));
        next.extend(working.into_iter().skip(1));
        working = next;
        chain = next_chain;
        active_definitions.insert(next_index);
    }

    if let Some(target) = working.first().and_then(|token| token.to_str()) {
        if !target.starts_with('-') {
            let exact = real_names.iter().any(|name| name == target);
            let has_prefix = real_names.iter().any(|name| name.starts_with(target));
            if !exact && !has_prefix {
                return Err(RagtagError::AliasTargetUnknown {
                    target: target.to_string(),
                    chain,
                });
            }
        }
    }

    let configured_len = working.len() - original_suffix.len();
    Ok(Expansion {
        working,
        configured_len,
    })
}

/// Assembles argv for the single terminal clap parse without changing tokens.
pub fn assemble_terminal_argv(
    original: &[OsString],
    prefix_end: usize,
    expansion: Expansion,
) -> Vec<OsString> {
    original
        .iter()
        .take(prefix_end)
        .cloned()
        .chain(expansion.working)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an alias with concise test syntax.
    fn alias(names: &[&str], arguments: &[&str]) -> Alias {
        Alias {
            names: names.iter().map(|name| (*name).to_string()).collect(),
            arguments: arguments
                .iter()
                .map(|argument| (*argument).to_string())
                .collect(),
        }
    }

    #[test]
    fn scanner_preserves_indices_and_recognizes_only_exact_root_tokens() {
        let cases = [
            (vec!["ragtag", "alias", "--"], OuterScan::Command(1)),
            (
                vec![
                    "ragtag",
                    "--no-color",
                    "--config=x",
                    "--config",
                    "y",
                    "alias",
                    "--help",
                ],
                OuterScan::Command(5),
            ),
            (
                vec!["ragtag", "--config", "", "alias"],
                OuterScan::Command(3),
            ),
            (vec!["ragtag", "--", "alias"], OuterScan::Terminal),
            (
                vec!["ragtag", "--config", "--", "alias"],
                OuterScan::Terminal,
            ),
            (vec!["ragtag", "--bogus", "alias"], OuterScan::Terminal),
            (vec!["ragtag", "--no-col", "alias"], OuterScan::Terminal),
            (
                vec!["ragtag", "--configuration=x", "alias"],
                OuterScan::Terminal,
            ),
            (vec!["ragtag", "-h"], OuterScan::Terminal),
            (vec!["ragtag", "--help"], OuterScan::Terminal),
            (vec!["ragtag", "--version"], OuterScan::Terminal),
            (vec!["ragtag", "--config"], OuterScan::Terminal),
        ];

        for (args, expected) in cases {
            let args = args.into_iter().map(OsString::from).collect::<Vec<_>>();
            assert_eq!(scan_outer_command(&args), expected, "{args:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn scanner_treats_non_utf8_hyphen_token_as_an_option() {
        use std::os::unix::ffi::OsStringExt;

        let args = vec![
            OsString::from("ragtag"),
            OsString::from_vec(vec![b'-', 0xff]),
            OsString::from("alias"),
        ];
        assert_eq!(scan_outer_command(&args), OuterScan::Terminal);
    }

    #[test]
    fn outer_resolution_handles_exact_prefix_unknown_and_ordered_ambiguity() {
        let definitions = vec![
            alias(&["sum-total", "st"], &["summary"]),
            alias(&["sum-all"], &["summary"]),
        ];
        let aliases = AliasIndex::new(&definitions);
        let real = vec!["summary".to_string(), "query".to_string()];

        assert_eq!(
            resolve_outer_command(OsStr::new("summary"), &real, &aliases).unwrap(),
            OuterResolution::Real
        );
        assert_eq!(
            resolve_outer_command(OsStr::new("quer"), &real, &aliases).unwrap(),
            OuterResolution::Real
        );
        assert_eq!(
            resolve_outer_command(OsStr::new("st"), &real, &aliases).unwrap(),
            OuterResolution::Alias {
                definition_index: 0,
                selected_name: "st".to_string(),
            }
        );
        assert_eq!(
            resolve_outer_command(OsStr::new("sum-t"), &real, &aliases).unwrap(),
            OuterResolution::Alias {
                definition_index: 0,
                selected_name: "sum-total".to_string(),
            }
        );
        assert_eq!(
            resolve_outer_command(OsStr::new("missing"), &real, &aliases).unwrap(),
            OuterResolution::None
        );

        let error = resolve_outer_command(OsStr::new("sum"), &real, &aliases).unwrap_err();
        assert_eq!(
            error.to_string(),
            "error: alias command \"sum\" is ambiguous; candidates: summary, sum-total, sum-all"
        );

        let error = resolve_outer_command(OsStr::new("s"), &[], &aliases).unwrap_err();
        assert_eq!(
            error.to_string(),
            "error: alias command \"s\" is ambiguous; candidates: sum-total, sum-all"
        );

        let synonymous = vec![alias(&["active", "act"], &["summary"])];
        let synonymous = AliasIndex::new(&synonymous);
        assert_eq!(
            resolve_outer_command(OsStr::new("ac"), &[], &synonymous).unwrap(),
            OuterResolution::Alias {
                definition_index: 0,
                selected_name: "active".to_string(),
            }
        );

        let real_only = vec!["task".to_string(), "table".to_string()];
        assert_eq!(
            resolve_outer_command(OsStr::new("ta"), &real_only, &aliases).unwrap(),
            OuterResolution::Real
        );
    }

    #[test]
    fn every_synonym_maps_to_one_definition_identity() {
        let definitions = vec![
            alias(&["primary", "p", "peer"], &["summary"]),
            alias(&["other"], &["query"]),
        ];
        let aliases = AliasIndex::new(&definitions);
        assert_eq!(aliases.definition_for_name("primary"), Some(0));
        assert_eq!(aliases.definition_for_name("p"), Some(0));
        assert_eq!(aliases.definition_for_name("peer"), Some(0));
        assert_eq!(aliases.definition_for_name("other"), Some(1));
        assert_eq!(aliases.definition_for_name("missing"), None);
    }

    #[test]
    fn recursive_composition_preserves_inner_outer_suffix_order() {
        let definitions = vec![
            alias(&["outer"], &["middle", "--outer"]),
            alias(&["middle", "m"], &["inner", "--middle"]),
            alias(&["inner"], &["query", "task", "--inner"]),
        ];
        let aliases = AliasIndex::new(&definitions);
        let real = vec!["query".to_string()];
        let suffix = [OsString::from("--suffix")];
        let expansion = expand_alias(&aliases, 0, "outer", &suffix, &real).unwrap();
        assert_eq!(
            expansion.working,
            ["query", "task", "--inner", "--middle", "--outer", "--suffix"]
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn synonym_identity_cycle_precedes_depth_check() {
        let definitions = vec![alias(&["a", "alt-a"], &["alt-a"])];
        let aliases = AliasIndex::new(&definitions);
        let error = expand_alias(&aliases, 0, "a", &[], &["summary".to_string()]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "error: alias expansion cycle: a -> alt-a"
        );
    }

    #[test]
    fn direct_two_node_and_long_cycles_report_complete_spelling_chains() {
        for (definitions, expected) in [
            (vec![alias(&["a"], &["a"])], "a -> a"),
            (
                vec![alias(&["a"], &["b"]), alias(&["b"], &["a"])],
                "a -> b -> a",
            ),
            (
                vec![
                    alias(&["a"], &["b"]),
                    alias(&["b"], &["c"]),
                    alias(&["c"], &["d"]),
                    alias(&["d"], &["b"]),
                ],
                "a -> b -> c -> d -> b",
            ),
        ] {
            let aliases = AliasIndex::new(&definitions);
            let error = expand_alias(&aliases, 0, "a", &[], &["summary".to_string()]).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("error: alias expansion cycle: {expected}")
            );
        }
    }

    #[test]
    fn depth_and_token_boundaries_are_exact() {
        let mut definitions = Vec::new();
        for index in 0..MAX_ALIAS_EXPANSION_DEPTH {
            let target = if index + 1 == MAX_ALIAS_EXPANSION_DEPTH {
                "summary".to_string()
            } else {
                format!("a{}", index + 1)
            };
            definitions.push(Alias {
                names: vec![format!("a{index}")],
                arguments: vec![target],
            });
        }
        let aliases = AliasIndex::new(&definitions);
        assert!(expand_alias(&aliases, 0, "a0", &[], &["summary".to_string()]).is_ok());

        definitions.push(alias(&["terminal"], &["summary"]));
        definitions[MAX_ALIAS_EXPANSION_DEPTH - 1].arguments = vec!["terminal".to_string()];
        let aliases = AliasIndex::new(&definitions);
        let error = expand_alias(&aliases, 0, "a0", &[], &["summary".to_string()]).unwrap_err();
        assert!(matches!(
            error,
            RagtagError::AliasExpansionDepthExceeded { limit: 32, .. }
        ));

        let definitions = vec![Alias {
            names: vec!["wide".to_string()],
            arguments: vec!["summary".to_string(); MAX_EXPANDED_ARGUMENTS],
        }];
        let aliases = AliasIndex::new(&definitions);
        assert!(expand_alias(&aliases, 0, "wide", &[], &["summary".to_string()]).is_ok());
        let error = expand_alias(
            &aliases,
            0,
            "wide",
            &[OsString::from("overflow")],
            &["summary".to_string()],
        )
        .unwrap_err();
        assert!(matches!(
            error,
            RagtagError::AliasExpansionArgumentsExceeded { count: 4097, .. }
        ));
    }

    #[test]
    fn projected_count_saturates_arithmetic_failures() {
        assert_eq!(
            checked_projected_count(4095, 1, 2, &["a".to_string()]).unwrap(),
            4096
        );
        assert!(matches!(
            checked_projected_count(4096, 1, 2, &["a".to_string()]),
            Err(RagtagError::AliasExpansionArgumentsExceeded { count: 4097, .. })
        ));

        for result in [
            checked_projected_count(0, 1, 0, &["a".to_string()]),
            checked_projected_count(usize::MAX, 0, 1, &["a".to_string()]),
        ] {
            assert!(matches!(
                result,
                Err(RagtagError::AliasExpansionArgumentsExceeded {
                    count: usize::MAX,
                    ..
                })
            ));
        }
    }

    #[test]
    fn unknown_and_option_leading_targets_are_distinguished() {
        let unknown = vec![alias(&["unknown"], &["missing"])];
        let error = expand_alias(
            &AliasIndex::new(&unknown),
            0,
            "unknown",
            &[],
            &["summary".to_string()],
        )
        .unwrap_err();
        assert!(matches!(error, RagtagError::AliasTargetUnknown { .. }));

        let option = vec![alias(&["option"], &["--no-color", "summary"])];
        assert!(expand_alias(
            &AliasIndex::new(&option),
            0,
            "option",
            &[],
            &["summary".to_string()]
        )
        .is_ok());
    }

    #[test]
    fn recursive_lookup_is_exact_and_real_terminal_classification_stays_with_clap() {
        let definitions = vec![
            alias(&["outer-prefix"], &["in"]),
            alias(&["inner"], &["summary"]),
            alias(&["real-prefix"], &["sum"]),
            alias(&["real-exact"], &["task"]),
            alias(&["real-ambiguous"], &["qu"]),
        ];
        let aliases = AliasIndex::new(&definitions);
        let real = vec![
            "summary".to_string(),
            "task".to_string(),
            "query".to_string(),
            "quorum".to_string(),
        ];

        let error = expand_alias(&aliases, 0, "outer-prefix", &[], &real).unwrap_err();
        assert!(matches!(
            error,
            RagtagError::AliasTargetUnknown { target, chain }
                if target == "in" && chain == ["outer-prefix"]
        ));
        for (definition_index, selected_name) in
            [(2, "real-prefix"), (3, "real-exact"), (4, "real-ambiguous")]
        {
            assert!(
                expand_alias(&aliases, definition_index, selected_name, &[], &real).is_ok(),
                "{selected_name}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_suffix_is_preserved_until_the_terminal_clap_boundary() {
        use std::os::unix::ffi::OsStringExt;

        // Alias processing must not introduce a Unicode conversion. The
        // terminal command's clap value parser decides whether the preserved
        // token is accepted.
        let definitions = vec![
            alias(&["outer"], &["inner", "--outer"]),
            alias(&["inner"], &["summary", "--inner"]),
        ];
        let invalid = OsString::from_vec(vec![0xff, b'x']);
        let expansion = expand_alias(
            &AliasIndex::new(&definitions),
            0,
            "outer",
            std::slice::from_ref(&invalid),
            &["summary".to_string()],
        )
        .unwrap();
        let original = vec![OsString::from("ragtag"), OsString::from("outer"), invalid];
        let terminal = assemble_terminal_argv(&original, 1, expansion);
        assert_eq!(
            terminal.last().cloned().unwrap().into_vec(),
            vec![0xff, b'x']
        );
    }
}
