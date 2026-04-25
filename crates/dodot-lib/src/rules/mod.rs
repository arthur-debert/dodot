//! Rule types, pattern matching, and file scanning.
//!
//! A rule pairs a file pattern with a handler name. The [`Scanner`]
//! walks a pack directory and matches each file against the rule set.
//! Rules are checked in descending priority order; the first match
//! wins. Filter handlers (`ignore`, `skip`) sit at the highest priority
//! tier so a file the user wants dropped never gets claimed by a
//! precise mapping or the catchall.

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
    pub pattern: String,

    /// Handler to use for matching files (e.g. `"symlink"`, `"install"`).
    pub handler: String,

    /// Higher priority rules are checked first. Default is 0.
    pub priority: i32,

    /// Match the basename case-insensitively. Set by `mappings.skip`
    /// patterns so README/Readme/readme all hit the same rule without
    /// forcing every config writer to enumerate every casing. Defaults
    /// to false (case-sensitive, matches glob conventions).
    #[serde(default, skip_serializing_if = "is_false")]
    pub case_insensitive: bool,

    /// Handler-specific options passed through from config.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A raw file entry discovered during directory walking (before rule matching).
#[derive(Debug, Clone)]
pub struct PackEntry {
    /// Path relative to the pack root (e.g. `"vimrc"`, `"nvim/init.lua"`).
    pub relative_path: PathBuf,
    /// Absolute path to the file.
    pub absolute_path: PathBuf,
    /// Whether this entry is a directory.
    pub is_dir: bool,
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

    /// If this file was produced by a preprocessor, the original source path.
    /// `None` for regular (non-preprocessed) files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preprocessor_source: Option<PathBuf>,
}

// ── Grouping helpers ────────────────────────────────────────────

/// Groups rule matches by handler name.
pub fn group_by_handler(matches: &[RuleMatch]) -> HashMap<String, Vec<RuleMatch>> {
    let mut groups: HashMap<String, Vec<RuleMatch>> = HashMap::new();
    for m in matches {
        groups.entry(m.handler.clone()).or_default().push(m.clone());
    }
    groups
}

/// Returns handler names in execution order.
///
/// Order is driven by each handler's [`ExecutionPhase`]
/// (see [`crate::handlers::ExecutionPhase`] for the full phase list and
/// why each slot is where it is). The phase enum's declaration order
/// *is* the execution order — `Provision` → `Setup` → `PathExport` →
/// `ShellInit` → `Link`.
///
/// Handler names not present in the registry are placed last in
/// alphabetical order (they get ignored by the pipeline anyway).
///
/// [`ExecutionPhase`]: crate::handlers::ExecutionPhase
pub fn handler_execution_order(
    groups: &HashMap<String, Vec<RuleMatch>>,
    registry: &HashMap<String, Box<dyn crate::handlers::Handler + '_>>,
) -> Vec<String> {
    let mut names: Vec<String> = groups.keys().cloned().collect();
    names.sort_by(|a, b| {
        let pa = registry.get(a).map(|h| h.phase());
        let pb = registry.get(b).map(|h| h.phase());
        match (pa, pb) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(b),
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
    /// Mirror of [`Rule::case_insensitive`]. The pattern itself is
    /// already lowercased at compile time when this is true; the
    /// matcher lowercases the candidate filename to match.
    case_insensitive: bool,
    handler: String,
    priority: i32,
    options: HashMap<String, String>,
}

fn compile_rules(rules: &[Rule]) -> Vec<CompiledRule> {
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

fn matches_entry(pattern: &CompiledPattern, filename: &str, is_dir: bool) -> bool {
    match pattern {
        CompiledPattern::Exact(name) => filename == name,
        CompiledPattern::Glob(glob) => glob.matches(filename),
        CompiledPattern::Directory(dir_name) => is_dir && filename == dir_name,
    }
}

// ── Scanner ─────────────────────────────────────────────────────

/// Files that are always skipped during scanning.
pub const SPECIAL_FILES: &[&str] = &[".dodot.toml", ".dodotignore"];

/// Should this entry name be skipped at scan or handler-recursion time?
///
/// Combines the three always-on filters: dodot's own files
/// (`SPECIAL_FILES`) and the pack's `ignore` glob patterns. Hidden
/// files are NOT filtered here — the caller decides whether to skip
/// dotfiles (the scanner does, for the top-level walk; the symlink
/// handler's per-file fallback does not, since the user opted in).
pub fn should_skip_entry(name: &str, ignore_patterns: &[String]) -> bool {
    SPECIAL_FILES.contains(&name) || is_ignored(name, ignore_patterns)
}

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
    ///
    /// This is a convenience wrapper over [`walk_pack`] + [`match_entries`].
    pub fn scan_pack(
        &self,
        pack: &Pack,
        rules: &[Rule],
        pack_ignore: &[String],
    ) -> Result<Vec<RuleMatch>> {
        let entries = self.walk_pack(&pack.path, pack_ignore)?;
        Ok(self.match_entries(&entries, rules, &pack.name))
    }

    /// Walk a pack directory and return raw file entries.
    ///
    /// Skips hidden files (except `.config`), special files
    /// (`.dodot.toml`, `.dodotignore`), and files matching
    /// pack-level ignore patterns.
    /// Walk the pack's top-level children only.
    ///
    /// Returns depth-1 entries (files and directories directly under
    /// the pack root). Nested files/dirs are **not** returned — handlers
    /// that receive a directory entry decide internally whether and how
    /// to recurse (e.g. symlink falls back to per-file mode when
    /// `protected_paths` or `targets` reach inside the dir).
    ///
    /// Preprocessing is the one exception: it still needs to see nested
    /// files to discover templates (`*.tmpl`) and the like. Use
    /// [`Scanner::walk_pack_recursive`] for that use case.
    pub fn walk_pack(
        &self,
        pack_path: &Path,
        ignore_patterns: &[String],
    ) -> Result<Vec<PackEntry>> {
        let mut results = Vec::new();
        self.list_top_level(pack_path, ignore_patterns, &mut results)?;
        Ok(results)
    }

    /// Walk the pack recursively. Only used by the preprocessing pipeline.
    pub fn walk_pack_recursive(
        &self,
        pack_path: &Path,
        ignore_patterns: &[String],
    ) -> Result<Vec<PackEntry>> {
        let mut results = Vec::new();
        self.walk_dir(pack_path, pack_path, ignore_patterns, &mut results)?;
        Ok(results)
    }

    /// Match a list of entries against rules, returning rule matches.
    ///
    /// This is the second half of the scan pipeline: given raw entries
    /// (from [`walk_pack`] or from preprocessing), match each against
    /// the rule set to determine which handler processes it.
    pub fn match_entries(
        &self,
        entries: &[PackEntry],
        rules: &[Rule],
        pack_name: &str,
    ) -> Vec<RuleMatch> {
        let compiled = compile_rules(rules);
        // Compute once per scan rather than per file: when no rule
        // requested case-insensitive matching, match_file can skip the
        // per-entry `to_lowercase` allocation entirely.
        let has_ci_rules = compiled.iter().any(|r| r.case_insensitive);
        // Sort once per scan, then reuse the ordered slice for every entry.
        let mut sorted: Vec<&CompiledRule> = compiled.iter().collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut matches = Vec::new();

        for entry in entries {
            let filename = entry
                .relative_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Some(rule_match) = match_file(
                &sorted,
                has_ci_rules,
                &filename,
                entry.is_dir,
                &entry.relative_path,
                &entry.absolute_path,
                pack_name,
            ) {
                matches.push(rule_match);
            }
        }

        matches.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        matches
    }

    /// Enumerate the direct children of `pack_path`, skipping hidden,
    /// special, and ignored entries. No recursion.
    fn list_top_level(
        &self,
        pack_path: &Path,
        ignore_patterns: &[String],
        results: &mut Vec<PackEntry>,
    ) -> Result<()> {
        let entries = self.fs.read_dir(pack_path)?;

        for entry in entries {
            let name = &entry.name;

            if name.starts_with('.') && name != ".config" {
                continue;
            }
            if SPECIAL_FILES.contains(&name.as_str()) {
                continue;
            }
            if is_ignored(name, ignore_patterns) {
                continue;
            }

            let rel_path = entry
                .path
                .strip_prefix(pack_path)
                .unwrap_or(&entry.path)
                .to_path_buf();

            results.push(PackEntry {
                relative_path: rel_path,
                absolute_path: entry.path.clone(),
                is_dir: entry.is_dir,
            });
        }

        Ok(())
    }

    fn walk_dir(
        &self,
        base: &Path,
        dir: &Path,
        ignore_patterns: &[String],
        results: &mut Vec<PackEntry>,
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

            let rel_path = entry
                .path
                .strip_prefix(base)
                .unwrap_or(&entry.path)
                .to_path_buf();

            if entry.is_dir {
                // Add directory itself as a candidate (for path handler)
                results.push(PackEntry {
                    relative_path: rel_path.clone(),
                    absolute_path: entry.path.clone(),
                    is_dir: true,
                });
                // Recurse into subdirectories
                self.walk_dir(base, &entry.path, ignore_patterns, results)?;
            } else {
                results.push(PackEntry {
                    relative_path: rel_path,
                    absolute_path: entry.path.clone(),
                    is_dir: false,
                });
            }
        }

        Ok(())
    }
}

/// Match a single file against the compiled rules.
///
/// Walks rules in descending priority order; first match wins. There
/// is no separate "exclusion" phase — filter handlers (`ignore`, `skip`)
/// win because they sit at the highest priority tier set by
/// [`mappings_to_rules`](crate::config::mappings_to_rules), not because
/// the matcher knows their names.
fn match_file<'a>(
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
            });
        }
    }

    None
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
        Pack::new(name.into(), path, HandlerConfig::default())
    }

    fn default_rules() -> Vec<Rule> {
        vec![
            Rule {
                pattern: "bin/".into(),
                handler: "path".into(),
                priority: 10,
                case_insensitive: false,
                options: HashMap::new(),
            },
            Rule {
                pattern: "install.sh".into(),
                handler: "install".into(),
                priority: 10,
                case_insensitive: false,
                options: HashMap::new(),
            },
            Rule {
                pattern: "aliases.sh".into(),
                handler: "shell".into(),
                priority: 10,
                case_insensitive: false,
                options: HashMap::new(),
            },
            Rule {
                pattern: "profile.sh".into(),
                handler: "shell".into(),
                priority: 10,
                case_insensitive: false,
                options: HashMap::new(),
            },
            Rule {
                pattern: "Brewfile".into(),
                handler: "homebrew".into(),
                priority: 10,
                case_insensitive: false,
                options: HashMap::new(),
            },
            Rule {
                pattern: "*".into(),
                handler: "symlink".into(),
                priority: 0,
                case_insensitive: false,
                options: HashMap::new(),
            },
        ]
    }

    // ── Pattern matching unit tests ─────────────────────────────

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
    fn scan_pack_ignore_rule_outranks_catchall() {
        // The ignore filter handler at priority 100 wins over the
        // priority-0 catchall, and emits a match with handler="ignore".
        // Status display filters those out before rendering, but at the
        // matcher level they ARE matches — the catchall must not also
        // claim them.
        let env = TempEnvironment::builder()
            .pack("test")
            .file("good.txt", "yes")
            .file("bad.tmp", "no")
            .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", env.dotfiles_root.join("test"));

        let rules = vec![
            Rule {
                pattern: "*.tmp".into(),
                handler: "ignore".into(),
                priority: 100,
                case_insensitive: false,
                options: HashMap::new(),
            },
            Rule {
                pattern: "*".into(),
                handler: "symlink".into(),
                priority: 0,
                case_insensitive: false,
                options: HashMap::new(),
            },
        ];

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();

        let bad = matches
            .iter()
            .find(|m| m.relative_path.to_string_lossy() == "bad.tmp")
            .expect("bad.tmp must still appear as a match");
        assert_eq!(bad.handler, "ignore");

        let good = matches
            .iter()
            .find(|m| m.relative_path.to_string_lossy() == "good.txt")
            .expect("good.txt must appear as a match");
        assert_eq!(good.handler, "symlink");
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
            Rule {
                pattern: "*.sh".into(),
                handler: "generic-shell".into(),
                priority: 5,
                case_insensitive: false,
                options: HashMap::new(),
            },
            Rule {
                pattern: "aliases.sh".into(),
                handler: "specific-shell".into(),
                priority: 10,
                case_insensitive: false,
                options: HashMap::new(),
            },
            Rule {
                pattern: "*".into(),
                handler: "symlink".into(),
                priority: 0,
                case_insensitive: false,
                options: HashMap::new(),
            },
        ];

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler, "specific-shell");
    }

    #[test]
    fn skip_handler_matches_case_insensitively() {
        // Note: case-only filename variants like "README" and "Readme"
        // collide on case-insensitive filesystems (macOS HFS+/APFS
        // default), so the fixture uses different basenames + casings
        // to prove the match logic works across casings.
        let env = TempEnvironment::builder()
            .pack("test")
            .file("README", "x")
            .file("readme.md", "x")
            .file("License.txt", "x")
            .file("notes.md", "x")
            .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", env.dotfiles_root.join("test"));

        let rules = vec![
            Rule {
                pattern: "README".into(),
                handler: "skip".into(),
                priority: 50,
                case_insensitive: true,
                options: HashMap::new(),
            },
            Rule {
                pattern: "README.*".into(),
                handler: "skip".into(),
                priority: 50,
                case_insensitive: true,
                options: HashMap::new(),
            },
            Rule {
                pattern: "LICENSE.*".into(),
                handler: "skip".into(),
                priority: 50,
                case_insensitive: true,
                options: HashMap::new(),
            },
            Rule {
                pattern: "*".into(),
                handler: "symlink".into(),
                priority: 0,
                case_insensitive: false,
                options: HashMap::new(),
            },
        ];

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();
        let by_handler: std::collections::HashMap<&str, Vec<&str>> =
            matches.iter().fold(Default::default(), |mut acc, m| {
                acc.entry(m.handler.as_str())
                    .or_default()
                    .push(m.relative_path.to_str().unwrap());
                acc
            });

        let mut skipped = by_handler.get("skip").cloned().unwrap_or_default();
        skipped.sort();
        assert_eq!(skipped, vec!["License.txt", "README", "readme.md"]);

        let symlinked = by_handler.get("symlink").cloned().unwrap_or_default();
        assert_eq!(symlinked, vec!["notes.md"]);
    }

    #[test]
    fn skip_handler_outranks_precise_handler() {
        // Priority 50 (skip) must beat the priority 10 mappings routes
        // — otherwise a `mappings.shell = ["README.sh"]` mistake would
        // silently source a README as a shell file.
        let env = TempEnvironment::builder()
            .pack("test")
            .file("README.sh", "x")
            .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", env.dotfiles_root.join("test"));

        let rules = vec![
            Rule {
                pattern: "README.*".into(),
                handler: "skip".into(),
                priority: 50,
                case_insensitive: true,
                options: HashMap::new(),
            },
            Rule {
                pattern: "*.sh".into(),
                handler: "shell".into(),
                priority: 10,
                case_insensitive: false,
                options: HashMap::new(),
            },
        ];

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler, "skip");
    }

    #[test]
    fn ignore_rule_outranks_skip() {
        // mappings.ignore (priority 100) must win over mappings.skip
        // (priority 50) — silent-drop is the stronger signal than
        // listed-but-not-acted-on.
        let env = TempEnvironment::builder()
            .pack("test")
            .file("README.md", "x")
            .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("test", env.dotfiles_root.join("test"));

        let rules = vec![
            Rule {
                pattern: "README.md".into(),
                handler: "ignore".into(),
                priority: 100,
                case_insensitive: false,
                options: HashMap::new(),
            },
            Rule {
                pattern: "README.*".into(),
                handler: "skip".into(),
                priority: 50,
                case_insensitive: true,
                options: HashMap::new(),
            },
            Rule {
                pattern: "*".into(),
                handler: "symlink".into(),
                priority: 0,
                case_insensitive: false,
                options: HashMap::new(),
            },
        ];

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler, "ignore");
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

        let bin_match = matches
            .iter()
            .find(|m| m.relative_path.to_string_lossy() == "bin");
        assert!(bin_match.is_some(), "bin directory should match");
        assert_eq!(bin_match.unwrap().handler, "path");
        assert!(bin_match.unwrap().is_dir);
    }

    #[test]
    fn nested_install_sh_is_not_matched_by_install_rule() {
        // A file named install.sh that lives deep inside a directory
        // must NOT activate the install handler. Only a top-level
        // install.sh triggers it.
        let env = TempEnvironment::builder()
            .pack("sneaky")
            .file("config/install.sh", "echo boom")
            .done()
            .build();

        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("sneaky", env.dotfiles_root.join("sneaky"));
        let rules = default_rules();

        let matches = scanner.scan_pack(&pack, &rules, &[]).unwrap();

        assert!(
            !matches.iter().any(|m| m.handler == "install"),
            "nested install.sh should not route to install handler: {matches:?}"
        );
    }

    #[test]
    fn scan_pack_returns_only_top_level_entries() {
        // Under the top-level-only scanner, nested files are not surfaced
        // as individual matches. The containing dir is the matched entry;
        // handlers (symlink wholesale, path, …) decide how to recurse.
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

        let relpaths: Vec<String> = matches
            .iter()
            .map(|m| m.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(
            relpaths.iter().any(|p| p == "nvim"),
            "top-level nvim dir should match: {relpaths:?}"
        );
        assert!(
            !relpaths.iter().any(|p| p.contains('/')),
            "no nested paths expected: {relpaths:?}"
        );
    }

    // ── Grouping tests (from PR 5, kept) ────────────────────────

    #[test]
    fn group_by_handler_groups_correctly() {
        let matches = vec![
            RuleMatch {
                relative_path: "vimrc".into(),
                absolute_path: "/d/vim/vimrc".into(),
                pack: "vim".into(),
                handler: "symlink".into(),
                is_dir: false,
                options: HashMap::new(),
                preprocessor_source: None,
            },
            RuleMatch {
                relative_path: "aliases.sh".into(),
                absolute_path: "/d/vim/aliases.sh".into(),
                pack: "vim".into(),
                handler: "shell".into(),
                is_dir: false,
                options: HashMap::new(),
                preprocessor_source: None,
            },
            RuleMatch {
                relative_path: "gvimrc".into(),
                absolute_path: "/d/vim/gvimrc".into(),
                pack: "vim".into(),
                handler: "symlink".into(),
                is_dir: false,
                options: HashMap::new(),
                preprocessor_source: None,
            },
        ];

        let groups = group_by_handler(&matches);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["symlink"].len(), 2);
        assert_eq!(groups["shell"].len(), 1);
    }

    #[test]
    fn handler_execution_order_follows_phase_declaration() {
        let mut groups = HashMap::new();
        groups.insert("symlink".into(), vec![]);
        groups.insert("install".into(), vec![]);
        groups.insert("shell".into(), vec![]);
        groups.insert("homebrew".into(), vec![]);
        groups.insert("path".into(), vec![]);

        let fs = crate::fs::OsFs::new();
        let registry = crate::handlers::create_registry(&fs);
        let order = handler_execution_order(&groups, &registry);

        // Exact order matches ExecutionPhase declaration:
        // Provision(homebrew) -> Setup(install) -> PathExport(path)
        //   -> ShellInit(shell) -> Link(symlink)
        assert_eq!(
            order,
            vec!["homebrew", "install", "path", "shell", "symlink"]
        );
    }

    #[test]
    fn handler_execution_order_places_unknown_handlers_last() {
        let mut groups = HashMap::new();
        groups.insert("symlink".into(), vec![]);
        groups.insert("zzz-unknown".into(), vec![]);
        groups.insert("homebrew".into(), vec![]);

        let fs = crate::fs::OsFs::new();
        let registry = crate::handlers::create_registry(&fs);
        let order = handler_execution_order(&groups, &registry);

        // Known handlers keep phase order; unknown lands at the end.
        assert_eq!(order, vec!["homebrew", "symlink", "zzz-unknown"]);
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
            preprocessor_source: None,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("vimrc"));
        assert!(json.contains("symlink"));
        assert!(!json.contains("options"));
    }
}
