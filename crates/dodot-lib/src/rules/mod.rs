//! Rule types, pattern matching, and file scanning.
//!
//! A rule pairs a file pattern with a handler name. The [`Scanner`]
//! walks a pack directory and matches each file against the rule set.
//! Exclusion rules are checked first, then inclusion rules by priority
//! (descending). The first match wins.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fs::Fs;
use crate::packs::Pack;
use crate::Result;

// ── Types ───────────────────────────────────────────────────────

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

// ── Grouping helpers ────────────────────────────────────────────

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

// ── Pattern matching ────────────────────────────────────────────

/// A compiled pattern that can match filenames and directory names.
#[derive(Debug)]
enum CompiledPattern {
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
struct CompiledRule {
    pattern: CompiledPattern,
    is_exclusion: bool,
    handler: String,
    priority: i32,
    options: HashMap<String, String>,
}

fn compile_rules(rules: &[Rule]) -> Vec<CompiledRule> {
    rules
        .iter()
        .map(|rule| {
            let (raw_pattern, is_exclusion) = if let Some(rest) = rule.pattern.strip_prefix('!') {
                (rest.to_string(), true)
            } else {
                (rule.pattern.clone(), false)
            };

            let pattern = if raw_pattern.ends_with('/') {
                // Directory pattern
                let dir_name = raw_pattern.trim_end_matches('/').to_string();
                CompiledPattern::Directory(dir_name)
            } else if raw_pattern.contains('*') || raw_pattern.contains('?') || raw_pattern.contains('[') {
                // Glob pattern
                match glob::Pattern::new(&raw_pattern) {
                    Ok(p) => CompiledPattern::Glob(p),
                    Err(_) => CompiledPattern::Exact(raw_pattern),
                }
            } else {
                CompiledPattern::Exact(raw_pattern)
            };

            CompiledRule {
                pattern,
                is_exclusion,
                handler: rule.handler.clone(),
                priority: rule.priority,
                options: rule.options.clone(),
            }
        })
        .collect()
}

fn matches_entry(pattern: &CompiledPattern, filename: &str, is_dir: bool) -> bool {
    match pattern {
        CompiledPattern::Exact(name) => filename == name,
        CompiledPattern::Glob(glob) => glob.matches(filename),
        CompiledPattern::Directory(dir_name) => is_dir && filename == dir_name,
    }
}

// ── Scanner ─────────────────────────────────────────────────────

/// Files that are always skipped during scanning.
const SPECIAL_FILES: &[&str] = &[".dodot.toml", ".dodotignore"];

/// Scans pack directories and matches files against rules.
pub struct Scanner<'a> {
    fs: &'a dyn Fs,
}

impl<'a> Scanner<'a> {
    pub fn new(fs: &'a dyn Fs) -> Self {
        Self { fs }
    }

    /// Scan a pack directory and return all rule matches.
    ///
    /// Walks the pack directory (non-recursively for top-level, but
    /// directories matched by the directory pattern are included as
    /// single entries). Skips hidden files (except `.config`), special
    /// files (`.dodot.toml`, `.dodotignore`), and files matching
    /// pack-level ignore patterns.
    pub fn scan_pack(
        &self,
        pack: &Pack,
        rules: &[Rule],
        pack_ignore: &[String],
    ) -> Result<Vec<RuleMatch>> {
        let compiled = compile_rules(rules);
        let entries = self.walk_pack(&pack.path, pack_ignore)?;
        let mut matches = Vec::new();

        for (rel_path, abs_path, is_dir) in entries {
            let filename = rel_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Some(rule_match) =
                self.match_file(&compiled, &filename, is_dir, &rel_path, &abs_path, &pack.name)
            {
                matches.push(rule_match);
            }
        }

        matches.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(matches)
    }

    /// Walk pack directory, returning (relative_path, absolute_path, is_dir).
    fn walk_pack(
        &self,
        pack_path: &Path,
        ignore_patterns: &[String],
    ) -> Result<Vec<(PathBuf, PathBuf, bool)>> {
        let mut results = Vec::new();
        self.walk_dir(pack_path, pack_path, ignore_patterns, &mut results)?;
        Ok(results)
    }

    fn walk_dir(
        &self,
        base: &Path,
        dir: &Path,
        ignore_patterns: &[String],
        results: &mut Vec<(PathBuf, PathBuf, bool)>,
    ) -> Result<()> {
        let entries = self.fs.read_dir(dir)?;

        for entry in entries {
            let name = &entry.name;

            // Skip hidden files/dirs (except .config)
            if name.starts_with('.') && name != ".config" {
                continue;
            }

            // Skip special files
            if SPECIAL_FILES.contains(&name.as_str()) {
                continue;
            }

            // Skip ignored patterns
            if is_ignored(name, ignore_patterns) {
                continue;
            }

            let rel_path = entry.path.strip_prefix(base).unwrap_or(&entry.path).to_path_buf();

            if entry.is_dir {
                // Add directory itself as a candidate (for path handler)
                results.push((rel_path.clone(), entry.path.clone(), true));
                // Recurse into subdirectories
                self.walk_dir(base, &entry.path, ignore_patterns, results)?;
            } else {
                results.push((rel_path, entry.path.clone(), false));
            }
        }

        Ok(())
    }

    /// Match a single file against the compiled rules.
    ///
    /// 1. Check exclusion rules first — if any match, file is skipped.
    /// 2. Check inclusion rules by priority (descending), first match wins.
    fn match_file(
        &self,
        compiled: &[CompiledRule],
        filename: &str,
        is_dir: bool,
        rel_path: &Path,
        abs_path: &Path,
        pack: &str,
    ) -> Option<RuleMatch> {
        // Phase 1: check exclusions
        for rule in compiled {
            if rule.is_exclusion && matches_entry(&rule.pattern, filename, is_dir) {
                return None;
            }
        }

        // Phase 2: find first matching inclusion rule (sorted by priority desc)
        // We sort a copy so we don't modify the original
        let mut inclusion_rules: Vec<&CompiledRule> =
            compiled.iter().filter(|r| !r.is_exclusion).collect();
        inclusion_rules.sort_by(|a, b| b.priority.cmp(&a.priority));

        for rule in inclusion_rules {
            if matches_entry(&rule.pattern, filename, is_dir) {
                return Some(RuleMatch {
                    relative_path: rel_path.to_path_buf(),
                    absolute_path: abs_path.to_path_buf(),
                    pack: pack.to_string(),
                    handler: rule.handler.clone(),
                    is_dir,
                    options: rule.options.clone(),
                });
            }
        }

        None
    }
}

fn is_ignored(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if let Ok(glob) = glob::Pattern::new(pattern) {
            if glob.matches(name) {
                return true;
            }
        }
        // Exact match fallback
        if name == pattern {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::HandlerConfig;
    use crate::testing::TempEnvironment;

    fn make_pack(name: &str, path: PathBuf) -> Pack {
        Pack {
            name: name.into(),
            path,
            config: HandlerConfig::default(),
        }
    }

    fn default_rules() -> Vec<Rule> {
        vec![
            Rule { pattern: "bin/".into(), handler: "path".into(), priority: 10, options: HashMap::new() },
            Rule { pattern: "install.sh".into(), handler: "install".into(), priority: 10, options: HashMap::new() },
            Rule { pattern: "aliases.sh".into(), handler: "shell".into(), priority: 10, options: HashMap::new() },
            Rule { pattern: "profile.sh".into(), handler: "shell".into(), priority: 10, options: HashMap::new() },
            Rule { pattern: "Brewfile".into(), handler: "homebrew".into(), priority: 10, options: HashMap::new() },
            Rule { pattern: "*".into(), handler: "symlink".into(), priority: 0, options: HashMap::new() },
        ]
    }

    // ── Pattern matching unit tests ─────────────────────────────

    #[test]
    fn exact_match() {
        let compiled = compile_rules(&[Rule {
            pattern: "install.sh".into(),
            handler: "install".into(),
            priority: 0,
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
            options: HashMap::new(),
        }]);
        assert!(matches_entry(&compiled[0].pattern, "bin", true));
        assert!(!matches_entry(&compiled[0].pattern, "bin", false));
        assert!(!matches_entry(&compiled[0].pattern, "lib", true));
    }

    #[test]
    fn exclusion_prefix() {
        let compiled = compile_rules(&[Rule {
            pattern: "!*.tmp".into(),
            handler: "exclude".into(),
            priority: 100,
            options: HashMap::new(),
        }]);
        assert!(compiled[0].is_exclusion);
        assert!(matches_entry(&compiled[0].pattern, "scratch.tmp", false));
    }

    #[test]
    fn catchall_matches_everything() {
        let compiled = compile_rules(&[Rule {
            pattern: "*".into(),
            handler: "symlink".into(),
            priority: 0,
            options: HashMap::new(),
        }]);
        assert!(matches_entry(&compiled[0].pattern, "anything", false));
        assert!(matches_entry(&compiled[0].pattern, "vimrc", false));
    }

    // ── Scanner integration tests ───────────────────────────────

    #[test]
    fn scan_pack_basic() {
        let env = TempEnvironment::builder()
            .pack("vim")
                .file("vimrc", "set nocompatible")
                .file("gvimrc", "set guifont=Mono")
                .file("aliases.sh", "alias vi=vim")
                .file("install.sh", "#!/bin/sh\necho setup")
                .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("vim", env.dotfiles_root.join("vim"));
        let rules = default_rules();

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();

        let handler_map: HashMap<String, Vec<String>> = {
            let mut m: HashMap<String, Vec<String>> = HashMap::new();
            for rm in &matches {
                m.entry(rm.handler.clone())
                    .or_default()
                    .push(rm.relative_path.to_string_lossy().to_string());
            }
            m
        };

        assert_eq!(handler_map["install"], vec!["install.sh"]);
        assert_eq!(handler_map["shell"], vec!["aliases.sh"]);
        assert!(handler_map["symlink"].contains(&"gvimrc".to_string()));
        assert!(handler_map["symlink"].contains(&"vimrc".to_string()));
    }

    #[test]
    fn scan_pack_skips_hidden_files() {
        let env = TempEnvironment::builder()
            .pack("test")
                .file("visible", "yes")
                .file(".hidden", "no")
                .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", env.dotfiles_root.join("test"));
        let rules = default_rules();

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();
        let names: Vec<String> = matches
            .iter()
            .map(|m| m.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(names.contains(&"visible".to_string()));
        assert!(!names.contains(&".hidden".to_string()));
    }

    #[test]
    fn scan_pack_skips_special_files() {
        let env = TempEnvironment::builder()
            .pack("test")
                .file("normal", "yes")
                .config("[pack]\nignore = []")
                .done()
            .build();

        // Also manually create .dodotignore (even though it shouldn't be scanned)
        let pack_dir = env.dotfiles_root.join("test");
        env.fs
            .write_file(&pack_dir.join(".dodotignore"), b"")
            .unwrap();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", pack_dir);
        let rules = default_rules();

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();
        let names: Vec<String> = matches
            .iter()
            .map(|m| m.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(names.contains(&"normal".to_string()));
        assert!(!names.contains(&".dodot.toml".to_string()));
        assert!(!names.contains(&".dodotignore".to_string()));
    }

    #[test]
    fn scan_pack_with_ignore_patterns() {
        let env = TempEnvironment::builder()
            .pack("test")
                .file("keep.txt", "yes")
                .file("skip.bak", "no")
                .file("other.bak", "no")
                .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", env.dotfiles_root.join("test"));
        let rules = default_rules();

        let matches = scanner
            .scan_pack(&pack, &rules, &["*.bak".to_string()])
            .unwrap();
        let names: Vec<String> = matches
            .iter()
            .map(|m| m.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(names.contains(&"keep.txt".to_string()));
        assert!(!names.contains(&"skip.bak".to_string()));
        assert!(!names.contains(&"other.bak".to_string()));
    }

    #[test]
    fn scan_pack_exclusion_rules_override_catchall() {
        let env = TempEnvironment::builder()
            .pack("test")
                .file("good.txt", "yes")
                .file("bad.tmp", "no")
                .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", env.dotfiles_root.join("test"));

        let rules = vec![
            Rule { pattern: "!*.tmp".into(), handler: "exclude".into(), priority: 100, options: HashMap::new() },
            Rule { pattern: "*".into(), handler: "symlink".into(), priority: 0, options: HashMap::new() },
        ];

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();
        let names: Vec<String> = matches
            .iter()
            .map(|m| m.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(names.contains(&"good.txt".to_string()));
        assert!(!names.contains(&"bad.tmp".to_string()));
    }

    #[test]
    fn scan_pack_priority_ordering() {
        let env = TempEnvironment::builder()
            .pack("test")
                .file("aliases.sh", "# shell")
                .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", env.dotfiles_root.join("test"));

        // Both *.sh and aliases.sh match — higher priority should win
        let rules = vec![
            Rule { pattern: "*.sh".into(), handler: "generic-shell".into(), priority: 5, options: HashMap::new() },
            Rule { pattern: "aliases.sh".into(), handler: "specific-shell".into(), priority: 10, options: HashMap::new() },
            Rule { pattern: "*".into(), handler: "symlink".into(), priority: 0, options: HashMap::new() },
        ];

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler, "specific-shell");
    }

    #[test]
    fn scan_pack_directory_entry() {
        let env = TempEnvironment::builder()
            .pack("test")
                .file("bin/my-script", "#!/bin/sh")
                .file("normal", "x")
                .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", env.dotfiles_root.join("test"));
        let rules = default_rules();

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();

        let bin_match = matches.iter().find(|m| m.relative_path.to_string_lossy() == "bin");
        assert!(bin_match.is_some(), "bin directory should match");
        assert_eq!(bin_match.unwrap().handler, "path");
        assert!(bin_match.unwrap().is_dir);
    }

    #[test]
    fn scan_pack_nested_files() {
        let env = TempEnvironment::builder()
            .pack("nvim")
                .file("nvim/init.lua", "require('config')")
                .file("nvim/lua/plugins.lua", "return {}")
                .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("nvim", env.dotfiles_root.join("nvim"));
        let rules = default_rules();

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();

        let file_matches: Vec<String> = matches
            .iter()
            .filter(|m| !m.is_dir)
            .map(|m| m.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(file_matches.contains(&"nvim/init.lua".to_string()));
        assert!(file_matches.contains(&"nvim/lua/plugins.lua".to_string()));
    }

    // ── Grouping tests (from PR 5, kept) ────────────────────────

    #[test]
    fn group_by_handler_groups_correctly() {
        let matches = vec![
            RuleMatch { relative_path: "vimrc".into(), absolute_path: "/d/vim/vimrc".into(), pack: "vim".into(), handler: "symlink".into(), is_dir: false, options: HashMap::new() },
            RuleMatch { relative_path: "aliases.sh".into(), absolute_path: "/d/vim/aliases.sh".into(), pack: "vim".into(), handler: "shell".into(), is_dir: false, options: HashMap::new() },
            RuleMatch { relative_path: "gvimrc".into(), absolute_path: "/d/vim/gvimrc".into(), pack: "vim".into(), handler: "symlink".into(), is_dir: false, options: HashMap::new() },
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

        let install_pos = order.iter().position(|n| n == "install").unwrap();
        let homebrew_pos = order.iter().position(|n| n == "homebrew").unwrap();
        let symlink_pos = order.iter().position(|n| n == "symlink").unwrap();
        let shell_pos = order.iter().position(|n| n == "shell").unwrap();
        let path_pos = order.iter().position(|n| n == "path").unwrap();

        assert!(install_pos < symlink_pos);
        assert!(homebrew_pos < shell_pos);
        assert!(homebrew_pos < path_pos);
        assert!(homebrew_pos < install_pos);
        assert!(path_pos < shell_pos);
        assert!(shell_pos < symlink_pos);
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
        assert!(!json.contains("options"));
    }
}
