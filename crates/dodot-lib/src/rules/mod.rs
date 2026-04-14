//! Rule types for mapping files to handlers.
//!
//! A rule pairs a file pattern with a handler name. When scanning a
//! pack, each file is matched against the rule set. The first matching
//! rule determines which handler processes that file.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

/// A rule mapping a file pattern to a handler.
#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    /// Pattern to match against (e.g. `"install.sh"`, `"*.sh"`, `"bin/"`).
    /// Prefixed with `!` for exclusion rules (e.g. `"!*.tmp"`).
    pub pattern: String,

    /// Handler to use for matching files (e.g. `"symlink"`, `"install"`).
    pub handler: String,

    /// Higher priority rules are checked first. Default is 0.
    pub priority: i32,

    /// Handler-specific options passed through from config.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,
}

/// A file that matched a rule during pack scanning.
#[derive(Debug, Clone, Serialize)]
pub struct RuleMatch {
    /// Path relative to the pack root (e.g. `"vimrc"`, `"nvim/init.lua"`).
    pub relative_path: PathBuf,

    /// Absolute path to the file.
    pub absolute_path: PathBuf,

    /// Name of the pack this file belongs to.
    pub pack: String,

    /// Name of the handler that should process this file.
    pub handler: String,

    /// Whether this entry is a directory.
    pub is_dir: bool,

    /// Handler-specific options from the matched rule.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,
}

/// Groups rule matches by handler name.
pub fn group_by_handler(matches: &[RuleMatch]) -> HashMap<String, Vec<RuleMatch>> {
    let mut groups: HashMap<String, Vec<RuleMatch>> = HashMap::new();
    for m in matches {
        groups
            .entry(m.handler.clone())
            .or_default()
            .push(m.clone());
    }
    groups
}

/// Returns handler names in execution order.
///
/// Code execution handlers (install, homebrew) run **first** so that
/// provisioning happens before config linking. Within each category,
/// handlers are sorted alphabetically for determinism.
pub fn handler_execution_order(groups: &HashMap<String, Vec<RuleMatch>>) -> Vec<String> {
    use crate::handlers::{
        HandlerCategory, HANDLER_HOMEBREW, HANDLER_INSTALL, HANDLER_PATH, HANDLER_SHELL,
        HANDLER_SYMLINK,
    };

    fn category_of(name: &str) -> HandlerCategory {
        match name {
            HANDLER_INSTALL | HANDLER_HOMEBREW => HandlerCategory::CodeExecution,
            HANDLER_SYMLINK | HANDLER_SHELL | HANDLER_PATH => HandlerCategory::Configuration,
            _ => HandlerCategory::Configuration,
        }
    }

    let mut names: Vec<String> = groups.keys().cloned().collect();
    names.sort_by(|a, b| {
        let cat_a = category_of(a);
        let cat_b = category_of(b);
        // CodeExecution first, then Configuration
        match (cat_a, cat_b) {
            (HandlerCategory::CodeExecution, HandlerCategory::Configuration) => {
                std::cmp::Ordering::Less
            }
            (HandlerCategory::Configuration, HandlerCategory::CodeExecution) => {
                std::cmp::Ordering::Greater
            }
            _ => a.cmp(b),
        }
    });
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_by_handler_groups_correctly() {
        let matches = vec![
            RuleMatch {
                relative_path: "vimrc".into(),
                absolute_path: "/dots/vim/vimrc".into(),
                pack: "vim".into(),
                handler: "symlink".into(),
                is_dir: false,
                options: HashMap::new(),
            },
            RuleMatch {
                relative_path: "aliases.sh".into(),
                absolute_path: "/dots/vim/aliases.sh".into(),
                pack: "vim".into(),
                handler: "shell".into(),
                is_dir: false,
                options: HashMap::new(),
            },
            RuleMatch {
                relative_path: "gvimrc".into(),
                absolute_path: "/dots/vim/gvimrc".into(),
                pack: "vim".into(),
                handler: "symlink".into(),
                is_dir: false,
                options: HashMap::new(),
            },
        ];

        let groups = group_by_handler(&matches);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["symlink"].len(), 2);
        assert_eq!(groups["shell"].len(), 1);
    }

    #[test]
    fn handler_execution_order_code_first() {
        let mut groups = HashMap::new();
        groups.insert("symlink".into(), vec![]);
        groups.insert("install".into(), vec![]);
        groups.insert("shell".into(), vec![]);
        groups.insert("homebrew".into(), vec![]);
        groups.insert("path".into(), vec![]);

        let order = handler_execution_order(&groups);

        // Code execution handlers first
        let install_pos = order.iter().position(|n| n == "install").unwrap();
        let homebrew_pos = order.iter().position(|n| n == "homebrew").unwrap();
        let symlink_pos = order.iter().position(|n| n == "symlink").unwrap();
        let shell_pos = order.iter().position(|n| n == "shell").unwrap();
        let path_pos = order.iter().position(|n| n == "path").unwrap();

        assert!(
            install_pos < symlink_pos,
            "install should come before symlink"
        );
        assert!(
            homebrew_pos < shell_pos,
            "homebrew should come before shell"
        );
        assert!(
            homebrew_pos < path_pos,
            "homebrew should come before path"
        );

        // Within code execution, alphabetical
        assert!(homebrew_pos < install_pos, "homebrew < install alphabetically");

        // Within configuration, alphabetical
        assert!(path_pos < shell_pos, "path < shell alphabetically");
        assert!(shell_pos < symlink_pos, "shell < symlink alphabetically");
    }

    #[test]
    fn rule_match_serializes() {
        let m = RuleMatch {
            relative_path: "vimrc".into(),
            absolute_path: "/dots/vim/vimrc".into(),
            pack: "vim".into(),
            handler: "symlink".into(),
            is_dir: false,
            options: HashMap::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("vimrc"));
        assert!(json.contains("symlink"));
        // Empty options should be omitted
        assert!(!json.contains("options"));
    }
}
