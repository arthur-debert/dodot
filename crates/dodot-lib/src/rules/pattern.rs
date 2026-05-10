//! Pattern compilation and per-file matching.
//!
//! Compiles raw [`Rule`]s into [`CompiledRule`]s and runs the
//! priority-sorted match against a single file. Used by the
//! [`Scanner`](crate::rules::Scanner) per entry it walks.

use std::collections::HashMap;
use std::path::Path;

use crate::rules::{Rule, RuleMatch};

/// A compiled pattern that can match filenames and directory names.
#[derive(Debug)]
pub(super) enum CompiledPattern {
    /// Exact filename match (e.g. `"install.sh"`).
    Exact(String),
    /// Glob match (e.g. `"*.sh"`).
    Glob(glob::Pattern),
    /// Directory match (e.g. `"bin/"` or `"bin"`). Matches directories
    /// whose name equals the given string.
    Directory(String),
}

/// A rule compiled for efficient matching.
#[derive(Debug)]
pub(super) struct CompiledRule {
    pub(super) pattern: CompiledPattern,
    /// Mirror of [`Rule::case_insensitive`]. The pattern itself is
    /// already lowercased at compile time when this is true; the
    /// matcher lowercases the candidate filename to match.
    pub(super) case_insensitive: bool,
    pub(super) handler: String,
    pub(super) priority: i32,
    pub(super) options: HashMap<String, String>,
}

pub(super) fn compile_rules(rules: &[Rule]) -> Vec<CompiledRule> {
    rules
        .iter()
        .map(|rule| {
            let raw_pattern = rule.pattern.clone();
            let case_insensitive = rule.case_insensitive;

            // For case-insensitive rules, lowercase the pattern at
            // compile time; the matcher lowercases the filename to mirror.
            let normalized = if case_insensitive {
                raw_pattern.to_lowercase()
            } else {
                raw_pattern
            };

            let pattern = if normalized.ends_with('/') {
                // Directory pattern
                let dir_name = normalized.trim_end_matches('/').to_string();
                CompiledPattern::Directory(dir_name)
            } else if normalized.contains('*')
                || normalized.contains('?')
                || normalized.contains('[')
            {
                // Glob pattern
                match glob::Pattern::new(&normalized) {
                    Ok(p) => CompiledPattern::Glob(p),
                    Err(_) => CompiledPattern::Exact(normalized),
                }
            } else {
                CompiledPattern::Exact(normalized)
            };

            CompiledRule {
                pattern,
                case_insensitive,
                handler: rule.handler.clone(),
                priority: rule.priority,
                options: rule.options.clone(),
            }
        })
        .collect()
}

pub(super) fn matches_entry(pattern: &CompiledPattern, filename: &str, is_dir: bool) -> bool {
    match pattern {
        CompiledPattern::Exact(name) => filename == name,
        CompiledPattern::Glob(glob) => glob.matches(filename),
        CompiledPattern::Directory(dir_name) => is_dir && filename == dir_name,
    }
}

/// Match a single file against the compiled rules.
///
/// Walks rules in descending priority order; first match wins. There
/// is no separate "exclusion" phase — filter handlers (`ignore`, `skip`)
/// win because they sit at the highest priority tier set by
/// [`mappings_to_rules`](crate::config::mappings_to_rules), not because
/// the matcher knows their names.
pub(super) fn match_file<'a>(
    sorted: &'a [&'a CompiledRule],
    has_ci_rules: bool,
    filename: &str,
    is_dir: bool,
    rel_path: &Path,
    abs_path: &Path,
    pack: &str,
) -> Option<RuleMatch> {
    // Only allocate the lowercased form when at least one rule actually
    // wants case-insensitive matching. The common case — no `skip`
    // rules at all, or the user has set `mappings.skip = []` — pays
    // zero allocations here.
    let lowered = if has_ci_rules {
        Some(filename.to_lowercase())
    } else {
        None
    };
    let pick = |rule: &CompiledRule| -> &str {
        if rule.case_insensitive {
            lowered.as_deref().unwrap_or(filename)
        } else {
            filename
        }
    };

    for rule in sorted {
        if matches_entry(&rule.pattern, pick(rule), is_dir) {
            return Some(RuleMatch {
                relative_path: rel_path.to_path_buf(),
                absolute_path: abs_path.to_path_buf(),
                pack: pack.to_string(),
                handler: rule.handler.clone(),
                is_dir,
                options: rule.options.clone(),
                preprocessor_source: None,
                rendered_bytes: None,
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let compiled = compile_rules(&[Rule {
            pattern: "install.sh".into(),
            handler: "install".into(),
            priority: 0,
            case_insensitive: false,
            options: HashMap::new(),
        }]);
        assert!(matches_entry(&compiled[0].pattern, "install.sh", false));
        assert!(!matches_entry(&compiled[0].pattern, "other.sh", false));
    }

    #[test]
    fn glob_match() {
        let compiled = compile_rules(&[Rule {
            pattern: "*.sh".into(),
            handler: "shell".into(),
            priority: 0,
            case_insensitive: false,
            options: HashMap::new(),
        }]);
        assert!(matches_entry(&compiled[0].pattern, "aliases.sh", false));
        assert!(matches_entry(&compiled[0].pattern, "profile.sh", false));
        assert!(!matches_entry(&compiled[0].pattern, "vimrc", false));
    }

    #[test]
    fn directory_match() {
        let compiled = compile_rules(&[Rule {
            pattern: "bin/".into(),
            handler: "path".into(),
            priority: 0,
            case_insensitive: false,
            options: HashMap::new(),
        }]);
        assert!(matches_entry(&compiled[0].pattern, "bin", true));
        assert!(!matches_entry(&compiled[0].pattern, "bin", false));
        assert!(!matches_entry(&compiled[0].pattern, "lib", true));
    }

    #[test]
    fn case_insensitive_pattern_lowercases_at_compile_time() {
        let compiled = compile_rules(&[Rule {
            pattern: "README.*".into(),
            handler: "skip".into(),
            priority: 50,
            case_insensitive: true,
            options: HashMap::new(),
        }]);
        assert!(compiled[0].case_insensitive);
        // Pattern is lowercased at compile time; matcher lowercases the
        // candidate filename to mirror.
        assert!(matches_entry(&compiled[0].pattern, "readme.md", false));
    }

    #[test]
    fn catchall_matches_everything() {
        let compiled = compile_rules(&[Rule {
            pattern: "*".into(),
            handler: "symlink".into(),
            priority: 0,
            case_insensitive: false,
            options: HashMap::new(),
        }]);
        assert!(matches_entry(&compiled[0].pattern, "anything", false));
        assert!(matches_entry(&compiled[0].pattern, "vimrc", false));
    }
}
