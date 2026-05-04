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
use std::sync::Arc;

use serde::Serialize;

use crate::fs::Fs;
use crate::gates::{parse_basename_gate, BasenameGate, GateTable, HostFacts};
use crate::handlers::HANDLER_GATE;
use crate::packs::Pack;
use crate::{DodotError, Result};

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
    /// When `Some`, this entry was gated out by a directory-segment
    /// gate (`_<label>/`) whose predicate evaluated false on this host.
    /// The scanner emits the gate dir as a single PackEntry with this
    /// set; [`Scanner::match_entries`] converts it to a
    /// `gate`-handler match for the status renderer. `None` for all
    /// other entries.
    pub gate_failure: Option<GateFailure>,
}

/// Diagnostic snapshot of a failed directory-segment gate, attached to
/// a [`PackEntry`] when the scanner gates out a `_<label>/` directory.
#[derive(Debug, Clone)]
pub struct GateFailure {
    pub label: String,
    pub predicate: String,
    pub host: String,
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

    /// In-memory rendered bytes for preprocessor-produced files.
    ///
    /// Populated by `plan_pack_inner` from
    /// `PreprocessResult.rendered_bytes` so that handlers needing
    /// the rendered content for sentinel hashing (`install`,
    /// `homebrew`) can hash these bytes directly instead of reading
    /// the rendered file from disk. That decoupling is the
    /// structural enabler for §7.4 Passive mode (`dodot status`,
    /// `up --dry-run`), where the rendered file is intentionally
    /// not written to disk. See issue #121.
    ///
    /// `None` for regular (non-preprocessed) files; handlers
    /// targeting those still read from `absolute_path` directly.
    /// `Arc<[u8]>` so cloning a `RuleMatch` (e.g. during handler
    /// grouping) doesn't duplicate the buffer.
    #[serde(skip)]
    pub rendered_bytes: Option<Arc<[u8]>>,
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
    /// `mappings_gates` is the `[mappings.gates]` glob → label map; pass
    /// an empty `HashMap` if not used.
    pub fn scan_pack(
        &self,
        pack: &Pack,
        rules: &[Rule],
        pack_ignore: &[String],
        gates: &GateTable,
        host: &HostFacts,
        mappings_gates: &HashMap<String, String>,
    ) -> Result<Vec<RuleMatch>> {
        let entries = self.walk_pack(&pack.path, pack_ignore, gates, host)?;
        self.match_entries(&entries, rules, &pack.name, gates, host, mappings_gates)
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
        gates: &GateTable,
        host: &HostFacts,
    ) -> Result<Vec<PackEntry>> {
        let mut results = Vec::new();
        self.list_top_level(pack_path, ignore_patterns, gates, host, &mut results)?;
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
    ///
    /// # Gates
    ///
    /// Each entry's basename is inspected for a gate token (`._<label>`).
    /// When found:
    ///
    /// - **Unknown label** → hard error (typo guard).
    /// - **Predicate true on this host** → the suffix is stripped from
    ///   the basename and `relative_path`; rule matching uses the
    ///   stripped name. The resulting [`RuleMatch::absolute_path`] still
    ///   points at the original on-disk file (with the suffix).
    /// - **Predicate false on this host** → a `RuleMatch` with
    ///   `handler = "gate"` is emitted, carrying the original (gated)
    ///   path and gate metadata in `options` for the status renderer.
    ///   The entry never reaches the rule matcher.
    ///
    /// See [`crate::gates`] for the grammar and semantics; the design
    /// rationale is in `docs/proposals/conditional-running.lex`.
    pub fn match_entries(
        &self,
        entries: &[PackEntry],
        rules: &[Rule],
        pack_name: &str,
        gates: &GateTable,
        host: &HostFacts,
        mappings_gates: &HashMap<String, String>,
    ) -> Result<Vec<RuleMatch>> {
        let compiled = compile_rules(rules);
        // Compute once per scan rather than per file: when no rule
        // requested case-insensitive matching, match_file can skip the
        // per-entry `to_lowercase` allocation entirely.
        let has_ci_rules = compiled.iter().any(|r| r.case_insensitive);
        // Sort once per scan, then reuse the ordered slice for every entry.
        let mut sorted: Vec<&CompiledRule> = compiled.iter().collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Compile [mappings.gates] globs once per scan. Each entry pairs
        // a glob pattern with the gate label it carries. Sort by the
        // raw pattern string (lexicographic) so iteration order is
        // deterministic across platforms — `HashMap` iteration is not.
        // First-match-wins on this sorted view; ties are settled by
        // the conflict guard against filename gates further below.
        // Invalid glob patterns are a hard error: silently dropping
        // them turns a typo into "no gate" with no diagnostic, which
        // is exactly the kind of debugging trap the typo-guard pattern
        // exists to prevent.
        let mut compiled_mapping_gates: Vec<(glob::Pattern, &str, &str)> =
            Vec::with_capacity(mappings_gates.len());
        for (pat, label) in mappings_gates {
            let compiled = glob::Pattern::new(pat).map_err(|e| {
                DodotError::Config(format!(
                    "invalid `[mappings.gates]` glob `{pat}` in pack `{pack_name}`: {e}"
                ))
            })?;
            compiled_mapping_gates.push((compiled, label.as_str(), pat.as_str()));
        }
        compiled_mapping_gates.sort_by(|a, b| a.2.cmp(b.2));
        let compiled_mapping_gates: Vec<(glob::Pattern, &str)> = compiled_mapping_gates
            .into_iter()
            .map(|(p, l, _)| (p, l))
            .collect();

        let mut matches = Vec::new();

        for entry in entries {
            // Directory-segment gate failure (C2) — the scanner has
            // already evaluated and decided "drop." Convert to a
            // gate-handler match for status visibility.
            if let Some(failure) = &entry.gate_failure {
                let mut options = HashMap::new();
                options.insert("gate_label".into(), failure.label.clone());
                options.insert("gate_predicate".into(), failure.predicate.clone());
                options.insert("gate_host".into(), failure.host.clone());
                matches.push(RuleMatch {
                    relative_path: entry.relative_path.clone(),
                    absolute_path: entry.absolute_path.clone(),
                    pack: pack_name.to_string(),
                    handler: HANDLER_GATE.into(),
                    is_dir: entry.is_dir,
                    options,
                    preprocessor_source: None,
                    rendered_bytes: None,
                });
                continue;
            }

            let filename = entry
                .relative_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // C4: `[mappings.gates]` glob check. First-match-wins
            // against the user-defined glob → label table. Conflicts
            // with a filename gate on the same file are a hard error.
            let rel_str = entry.relative_path.to_string_lossy();
            let mapping_gate_label: Option<&str> = compiled_mapping_gates
                .iter()
                .find(|(pat, _)| pat.matches(&rel_str))
                .map(|(_, label)| *label);

            let basename_gate = parse_basename_gate(&filename);

            if let Some(map_label) = mapping_gate_label {
                if matches!(basename_gate, BasenameGate::Found { .. }) {
                    return Err(DodotError::Config(format!(
                        "gate-routing conflict in pack `{pack_name}` for `{}`: \
                         file carries both a filename gate token (`._<label>`) \
                         and a `[mappings.gates]` entry (`{map_label}`). \
                         Pick one — either rename the file (drop the suffix) \
                         or remove the `[mappings.gates]` entry.",
                        entry.relative_path.display()
                    )));
                }
                let pred = gates.lookup(map_label).ok_or_else(|| {
                    DodotError::Config(format!(
                        "unknown gate label `{map_label}` referenced from \
                         `[mappings.gates]` in pack `{pack_name}`: label is \
                         not in the built-in seed and not defined in [gates]."
                    ))
                })?;
                if !pred.matches(host) {
                    let mut options = HashMap::new();
                    options.insert("gate_label".into(), map_label.to_string());
                    options.insert("gate_predicate".into(), pred.describe());
                    options.insert("gate_host".into(), describe_host_for_predicate(pred, host));
                    matches.push(RuleMatch {
                        relative_path: entry.relative_path.clone(),
                        absolute_path: entry.absolute_path.clone(),
                        pack: pack_name.to_string(),
                        handler: HANDLER_GATE.into(),
                        is_dir: entry.is_dir,
                        options,
                        preprocessor_source: None,
                        rendered_bytes: None,
                    });
                    continue;
                }
                // Pass: fall through to normal rule matching with the
                // unstripped filename.
            }

            // Gate evaluation: parse, look up label, evaluate.
            let (effective_filename, effective_rel_path) = match basename_gate {
                BasenameGate::None => (filename.clone(), entry.relative_path.clone()),
                BasenameGate::Found { label, stripped } => {
                    let pred = gates.lookup(label).ok_or_else(|| {
                        DodotError::Config(format!(
                            "unknown gate label `{label}` in pack `{pack_name}`, file `{}`: \
                             label is not in the built-in seed and not defined in [gates]. \
                             Built-ins: darwin, linux, macos, arm64, aarch64, x86_64.",
                            entry.relative_path.display()
                        ))
                    })?;
                    if pred.matches(host) {
                        // Pass: present the entry under its stripped name.
                        let stripped_rel = entry.relative_path.with_file_name(&stripped);
                        (stripped, stripped_rel)
                    } else {
                        // Fail: synthesise a gate match, carry metadata.
                        let mut options = HashMap::new();
                        options.insert("gate_label".into(), label.to_string());
                        options.insert("gate_predicate".into(), pred.describe());
                        options.insert("gate_host".into(), describe_host_for_predicate(pred, host));
                        matches.push(RuleMatch {
                            relative_path: entry.relative_path.clone(),
                            absolute_path: entry.absolute_path.clone(),
                            pack: pack_name.to_string(),
                            handler: HANDLER_GATE.into(),
                            is_dir: entry.is_dir,
                            options,
                            preprocessor_source: None,
                            rendered_bytes: None,
                        });
                        continue;
                    }
                }
            };

            if let Some(rule_match) = match_file(
                &sorted,
                has_ci_rules,
                &effective_filename,
                entry.is_dir,
                &effective_rel_path,
                &entry.absolute_path,
                pack_name,
            ) {
                matches.push(rule_match);
            }
        }

        matches.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(matches)
    }

    /// Enumerate the direct children of `pack_path`, skipping hidden,
    /// special, and ignored entries.
    ///
    /// Top-level directories matching the gate-dir grammar
    /// (`_<label>/` where `<label>` is not a routing prefix) are
    /// expanded transparently:
    ///
    /// - Gate **passes** → descend; the gate-dir's children surface at
    ///   the pack root with the `_<label>/` prefix stripped from their
    ///   relative paths. This is the C2 surface from
    ///   `docs/proposals/conditional-running.lex` §5.1.
    /// - Gate **fails** → emit a single [`PackEntry`] for the gate dir
    ///   with `gate_failure: Some(...)` so [`Self::match_entries`] can
    ///   surface it as a `gate`-handler match.
    /// - **Unknown label** → hard error (typo guard).
    fn list_top_level(
        &self,
        pack_path: &Path,
        ignore_patterns: &[String],
        gates: &GateTable,
        host: &HostFacts,
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

            // Directory-segment gate handling (C2). Routing prefixes
            // (`_home/` etc.) are excluded by `parse_dir_gate_label`.
            if entry.is_dir {
                if let Some(label) = crate::gates::parse_dir_gate_label(name) {
                    let pred = gates.lookup(label).ok_or_else(|| {
                        DodotError::Config(format!(
                            "unknown gate label `{label}` in directory `{}`: \
                             label is not in the built-in seed and not defined in [gates]. \
                             Built-ins: darwin, linux, macos, arm64, aarch64, x86_64.",
                            entry.path.display()
                        ))
                    })?;
                    if pred.matches(host) {
                        // Pass: expand transparently. Children of the
                        // gate dir surface at pack-root level with the
                        // gate segment stripped from their rel paths.
                        self.list_top_level(&entry.path, ignore_patterns, gates, host, results)?;
                    } else {
                        // Fail: emit the gate-dir entry with a marker.
                        results.push(PackEntry {
                            relative_path: rel_path,
                            absolute_path: entry.path.clone(),
                            is_dir: true,
                            gate_failure: Some(GateFailure {
                                label: label.to_string(),
                                predicate: pred.describe(),
                                host: describe_host_for_predicate(pred, host),
                            }),
                        });
                    }
                    continue;
                }
            }

            results.push(PackEntry {
                relative_path: rel_path,
                absolute_path: entry.path.clone(),
                is_dir: entry.is_dir,
                gate_failure: None,
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
                    gate_failure: None,
                });
                // Recurse into subdirectories
                self.walk_dir(base, &entry.path, ignore_patterns, results)?;
            } else {
                results.push(PackEntry {
                    relative_path: rel_path,
                    absolute_path: entry.path.clone(),
                    is_dir: false,
                    gate_failure: None,
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
                rendered_bytes: None,
            });
        }
    }

    None
}

/// Render the host's actual values for the dimensions a predicate
/// cares about, so the status renderer can show "expected vs actual"
/// without re-walking the predicate. Same compact shape as
/// `GatePredicate::describe`.
fn describe_host_for_predicate(pred: &crate::gates::GatePredicate, host: &HostFacts) -> String {
    let parts: Vec<String> = pred
        .matchers
        .iter()
        .map(|(dim, _)| {
            let actual = host.get(*dim).unwrap_or("<unset>");
            format!("{}={}", dim.as_str(), actual)
        })
        .collect();
    parts.join(", ")
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

    /// Stable test fixture: built-in gates + a darwin/aarch64 host.
    /// Scanner tests that don't care about gates pin a stable host so
    /// any incidental `._darwin.*` filename behaves identically.
    fn test_gates() -> (GateTable, HostFacts) {
        (
            GateTable::with_builtins(),
            HostFacts::for_tests("darwin", "aarch64"),
        )
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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();

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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();
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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();
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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(
                &pack,
                &rules,
                &["*.bak".to_string()],
                &gates,
                &host,
                &HashMap::new(),
            )
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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();

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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();
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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();
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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();
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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();
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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();

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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();

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

        let (gates, host) = test_gates();
        let matches = scanner
            .scan_pack(&pack, &rules, &[], &gates, &host, &HashMap::new())
            .unwrap();

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
                rendered_bytes: None,
            },
            RuleMatch {
                relative_path: "aliases.sh".into(),
                absolute_path: "/d/vim/aliases.sh".into(),
                pack: "vim".into(),
                handler: "shell".into(),
                is_dir: false,
                options: HashMap::new(),
                preprocessor_source: None,
                rendered_bytes: None,
            },
            RuleMatch {
                relative_path: "gvimrc".into(),
                absolute_path: "/d/vim/gvimrc".into(),
                pack: "vim".into(),
                handler: "symlink".into(),
                is_dir: false,
                options: HashMap::new(),
                preprocessor_source: None,
                rendered_bytes: None,
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
            rendered_bytes: None,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("vimrc"));
        assert!(json.contains("symlink"));
        assert!(!json.contains("options"));
    }

    // ── Gate integration tests ──────────────────────────────────

    fn host_pair(os: &str, arch: &str) -> (GateTable, HostFacts) {
        (GateTable::with_builtins(), HostFacts::for_tests(os, arch))
    }

    #[test]
    fn gate_passing_strips_suffix_and_routes_to_handler() {
        // install._darwin.sh on darwin → matches `install.sh` mapping,
        // routes to the install handler with stripped relative_path.
        let env = TempEnvironment::builder()
            .pack("mac")
            .file("install._darwin.sh", "#!/bin/sh\necho mac-only")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("mac", env.dotfiles_root.join("mac"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.handler, "install");
        assert_eq!(m.relative_path.to_string_lossy(), "install.sh");
        // absolute_path is the original on-disk file — install handler
        // executes the actual `install._darwin.sh` script.
        assert!(m
            .absolute_path
            .to_string_lossy()
            .ends_with("install._darwin.sh"));
    }

    #[test]
    fn gate_failing_emits_gate_handler_match() {
        // install._linux.sh on darwin → gate fails, surfaces under "gate".
        let env = TempEnvironment::builder()
            .pack("cross")
            .file("install._linux.sh", "#!/bin/sh\napt-get foo")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("cross", env.dotfiles_root.join("cross"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.handler, crate::handlers::HANDLER_GATE);
        // Gated entries keep their original path (with the suffix) so
        // status can render the source name truthfully.
        assert_eq!(m.relative_path.to_string_lossy(), "install._linux.sh");
        // Metadata for the status renderer.
        assert_eq!(m.options.get("gate_label"), Some(&"linux".to_string()));
        assert_eq!(
            m.options.get("gate_predicate"),
            Some(&"os=linux".to_string())
        );
        assert_eq!(m.options.get("gate_host"), Some(&"os=darwin".to_string()));
    }

    #[test]
    fn gate_unknown_label_is_hard_error() {
        let env = TempEnvironment::builder()
            .pack("typo")
            .file("install._darwn.sh", "#!/bin/sh") // typo: darwn
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("typo", env.dotfiles_root.join("typo"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let err = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("darwn"), "missing label: {msg}");
        assert!(msg.contains("typo"), "missing pack: {msg}");
        assert!(msg.contains("install._darwn.sh"), "missing file: {msg}");
    }

    #[test]
    fn gate_compound_user_label_evaluates_and() {
        // arm-mac requires darwin AND aarch64. Pass only when both match.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("setup._arm-mac.sh", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));

        // Build a table with `arm-mac` defined.
        let mut user = HashMap::new();
        let mut arm_mac = HashMap::new();
        arm_mac.insert("os".into(), "darwin".into());
        arm_mac.insert("arch".into(), "aarch64".into());
        user.insert("arm-mac".into(), arm_mac);

        // Case 1: darwin + aarch64 → pass.
        let mut gates = GateTable::with_builtins();
        gates.merge_user(&user).unwrap();
        let host = HostFacts::for_tests("darwin", "aarch64");

        // setup._arm-mac.sh strips to setup.sh, which doesn't match any
        // precise rule, so it falls through to the catchall symlink.
        let mut rules = default_rules();
        rules.push(Rule {
            pattern: "setup.sh".into(),
            handler: "shell".into(),
            priority: 10,
            case_insensitive: false,
            options: HashMap::new(),
        });

        let matches = scanner
            .match_entries(
                &scanner.walk_pack(&pack.path, &[], &gates, &host).unwrap(),
                &rules,
                &pack.name,
                &gates,
                &host,
                &HashMap::new(),
            )
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler, "shell");
        assert_eq!(matches[0].relative_path.to_string_lossy(), "setup.sh");

        // Case 2: darwin + x86_64 → fail (arch mismatch).
        let host_intel = HostFacts::for_tests("darwin", "x86_64");
        let matches = scanner
            .match_entries(
                &scanner
                    .walk_pack(&pack.path, &[], &gates, &host_intel)
                    .unwrap(),
                &rules,
                &pack.name,
                &gates,
                &host_intel,
                &HashMap::new(),
            )
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler, crate::handlers::HANDLER_GATE);
    }

    #[test]
    fn gate_composes_with_template_extension() {
        // aliases._darwin.sh.tmpl → strips to aliases.sh.tmpl. The
        // template preprocessor still fires on the surviving entry; the
        // matcher itself just sees the .tmpl extension.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("aliases._darwin.sh.tmpl", "alias x=y")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        // .tmpl is preserved → preprocessor will pick it up.
        assert_eq!(m.relative_path.to_string_lossy(), "aliases.sh.tmpl");
        // Falls through to the catchall (symlink) since `aliases.sh.tmpl`
        // isn't in `mappings.shell`. In the real pipeline this match
        // would be replaced by the preprocessor's rendered output before
        // dispatch — that's not the scanner's concern.
        assert_eq!(m.handler, "symlink");
    }

    #[test]
    fn gate_composes_with_home_routing_prefix() {
        // home.bashrc._darwin → strips to home.bashrc. The symlink
        // resolver then routes via the `home.X` priority-1 rule.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("home.bashrc._darwin", "# bashrc")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.relative_path.to_string_lossy(), "home.bashrc");
        assert_eq!(m.handler, "symlink");
    }

    #[test]
    fn gate_mixed_files_in_one_pack() {
        // A pack with darwin-only, linux-only, and unconditional files.
        // On darwin: darwin file passes, linux file is gated out,
        // unconditional file passes.
        let env = TempEnvironment::builder()
            .pack("cross")
            .file("install._darwin.sh", "#!/bin/sh\necho mac")
            .file("install._linux.sh", "#!/bin/sh\necho linux")
            .file("vimrc", "set nocompatible")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("cross", env.dotfiles_root.join("cross"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();

        let by_handler: HashMap<&str, Vec<String>> =
            matches.iter().fold(HashMap::new(), |mut acc, m| {
                acc.entry(m.handler.as_str())
                    .or_default()
                    .push(m.relative_path.to_string_lossy().to_string());
                acc
            });

        // install._darwin.sh stripped to install.sh → install handler.
        assert_eq!(
            by_handler.get("install"),
            Some(&vec!["install.sh".to_string()])
        );
        // install._linux.sh kept as-is → gate handler.
        assert_eq!(
            by_handler.get(crate::handlers::HANDLER_GATE),
            Some(&vec!["install._linux.sh".to_string()])
        );
        // vimrc → catchall symlink.
        assert_eq!(by_handler.get("symlink"), Some(&vec!["vimrc".to_string()]));
    }

    #[test]
    fn gate_brewfile_extensionless() {
        // Brewfile._darwin → strips to Brewfile, matches homebrew handler.
        let env = TempEnvironment::builder()
            .pack("brew")
            .file("Brewfile._darwin", "brew \"ripgrep\"")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("brew", env.dotfiles_root.join("brew"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler, "homebrew");
        assert_eq!(matches[0].relative_path.to_string_lossy(), "Brewfile");
    }

    #[test]
    fn gate_arch_label_uses_arm64_alias() {
        // arm64 is an alias for aarch64; built-in.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("aliases._arm64.sh", "alias x=y")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.relative_path.to_string_lossy(), "aliases.sh");
        assert_eq!(m.handler, "shell");
    }

    // ── C2: directory-segment gates ─────────────────────────────

    #[test]
    fn dir_gate_passing_descends_and_flattens() {
        // _darwin/foo.sh on darwin → surfaces as foo.sh at pack root,
        // gate dir is transparent.
        let env = TempEnvironment::builder()
            .pack("cross")
            .file("_darwin/macos.sh", "#!/bin/sh\necho mac")
            .file("shared", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("cross", env.dotfiles_root.join("cross"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        let names: Vec<String> = matches
            .iter()
            .map(|m| m.relative_path.to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"macos.sh".to_string()), "{names:?}");
        assert!(names.contains(&"shared".to_string()), "{names:?}");
        // The gate dir itself must NOT surface as an entry.
        assert!(!names.iter().any(|n| n.starts_with("_darwin")), "{names:?}");
    }

    #[test]
    fn dir_gate_failing_emits_gate_match() {
        // _linux/ on darwin → surfaces as a single gate-handler match
        // for the directory; its contents are not surfaced.
        let env = TempEnvironment::builder()
            .pack("cross")
            .file("_linux/linux.sh", "#!/bin/sh\necho linux")
            .file("shared", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("cross", env.dotfiles_root.join("cross"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();

        // Exactly two matches: the gate dir (failed) and `shared`.
        assert_eq!(matches.len(), 2, "{matches:?}");

        let gate_match = matches
            .iter()
            .find(|m| m.handler == crate::handlers::HANDLER_GATE)
            .expect("expected gate match");
        assert_eq!(gate_match.relative_path.to_string_lossy(), "_linux");
        assert!(gate_match.is_dir);
        assert_eq!(
            gate_match.options.get("gate_label"),
            Some(&"linux".to_string())
        );

        let shared = matches
            .iter()
            .find(|m| m.relative_path.to_string_lossy() == "shared")
            .expect("expected shared file");
        assert_eq!(shared.handler, "symlink");
    }

    #[test]
    fn dir_gate_routing_prefix_is_not_a_gate() {
        // _home/ is a routing-prefix dir, NOT a gate. The scanner
        // surfaces it as a regular top-level dir; the symlink handler
        // takes it from there.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("_home/.bashrc", "# bashrc")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.relative_path.to_string_lossy(), "_home");
        assert!(m.is_dir);
        assert_eq!(m.handler, "symlink");
    }

    #[test]
    fn dir_gate_unknown_label_is_hard_error() {
        let env = TempEnvironment::builder()
            .pack("typo")
            .file("_darwn/foo.sh", "x") // typo: darwn
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("typo", env.dotfiles_root.join("typo"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let err = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("darwn"), "missing label: {msg}");
        assert!(msg.contains("_darwn"), "missing dir name: {msg}");
    }

    #[test]
    fn dir_gate_nested_inside_passing_gate_still_evaluates() {
        // _darwin/_arm64/x.sh on darwin+aarch64 → both gates pass,
        // file surfaces as x.sh at pack root.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("_darwin/_arm64/install.sh", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.relative_path.to_string_lossy(), "install.sh");
        assert_eq!(m.handler, "install");
    }

    #[test]
    fn dir_gate_nested_failing_inner_gate_drops_subtree() {
        // _darwin/_x86_64/install.sh on darwin+aarch64 → outer passes,
        // inner _x86_64 fails → that subtree is dropped (gate match
        // for the inner dir).
        let env = TempEnvironment::builder()
            .pack("p")
            .file("_darwin/_x86_64/install.sh", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        // One gate match for the inner _x86_64 dir; install.sh does
        // NOT surface.
        let gate = matches
            .iter()
            .find(|m| m.handler == crate::handlers::HANDLER_GATE);
        assert!(gate.is_some(), "expected a gate match: {matches:?}");
        assert!(
            !matches
                .iter()
                .any(|m| m.relative_path.to_string_lossy() == "install.sh"),
            "install.sh must not deploy when its enclosing gate fails: {matches:?}"
        );
    }

    #[test]
    fn dir_gate_with_routing_prefix_inside_passing_gate() {
        // The proposed pattern: _darwin/_home/.bashrc on darwin →
        // gate passes, descent surfaces _home/.bashrc as a top-level
        // routing-prefix subtree (which the symlink resolver handles
        // via priority 2a).
        let env = TempEnvironment::builder()
            .pack("p")
            .file("_darwin/_home/.bashrc", "# bashrc")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &HashMap::new())
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        // After gate strip, the entry surfaces under `_home` at pack
        // root level, exactly as if the user wrote `_home/` directly.
        assert_eq!(m.relative_path.to_string_lossy(), "_home");
        assert!(m.is_dir);
        assert_eq!(m.handler, "symlink");
    }

    // ── C4: [mappings.gates] glob-based gating ─────────────────

    #[test]
    fn mappings_gate_failing_drops_file() {
        // [mappings.gates] = { "install-mac.sh" = "linux" } on darwin →
        // file is gated out.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("install-mac.sh", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let mut mappings_gates = HashMap::new();
        mappings_gates.insert("install-mac.sh".to_string(), "linux".to_string());

        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.handler, crate::handlers::HANDLER_GATE);
        assert_eq!(m.options.get("gate_label"), Some(&"linux".to_string()));
    }

    #[test]
    fn mappings_gate_passing_does_not_alter_dispatch() {
        // [mappings.gates] = { "install-mac.sh" = "darwin" } on darwin →
        // file passes; rule matching proceeds as if no gate were set.
        // install-mac.sh isn't in default install patterns so it falls
        // through to the catchall symlink — same as without the gate.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("install-mac.sh", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let mut mappings_gates = HashMap::new();
        mappings_gates.insert("install-mac.sh".to_string(), "darwin".to_string());

        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
            .unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.handler, "symlink");
        assert_eq!(m.relative_path.to_string_lossy(), "install-mac.sh");
    }

    #[test]
    fn mappings_gate_glob_matches_subpath() {
        // [mappings.gates] = { "setup/*.sh" = "linux" } on darwin →
        // setup/foo.sh is gated out.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("setup/foo.sh", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let mut mappings_gates = HashMap::new();
        mappings_gates.insert("setup/*.sh".to_string(), "linux".to_string());

        let matches = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
            .unwrap();
        // Top-level "setup" dir is what surfaces from walk_pack (per
        // the existing depth-1 contract). The mapping pattern matches
        // its child path "setup/foo.sh" — but the scanner only sees
        // "setup" as a top-level dir and the mapping doesn't match
        // that. So this test documents the C4 limit: globs are
        // matched against the relative path the scanner surfaces, not
        // against arbitrary nested paths the symlink handler would
        // recurse into.
        //
        // For the C4 v1 surface, glob-based gating works on top-level
        // entries. Files inside subdirectories deploy via the symlink
        // handler's wholesale link of the parent.
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler, "symlink");
        assert_eq!(matches[0].relative_path.to_string_lossy(), "setup");
    }

    #[test]
    fn mappings_gate_conflict_with_basename_gate_errors() {
        // File has BOTH a filename gate (`._darwin`) AND a
        // [mappings.gates] entry → hard error.
        let env = TempEnvironment::builder()
            .pack("p")
            .file("install._darwin.sh", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let mut mappings_gates = HashMap::new();
        mappings_gates.insert("install._darwin.sh".to_string(), "linux".to_string());

        let err = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("gate-routing conflict"), "{msg}");
        assert!(msg.contains("install._darwin.sh"), "{msg}");
    }

    #[test]
    fn mappings_gate_unknown_label_errors() {
        let env = TempEnvironment::builder()
            .pack("p")
            .file("foo.sh", "x")
            .done()
            .build();
        let scanner = Scanner::new(env.fs.as_ref());
        let pack = make_pack("p", env.dotfiles_root.join("p"));
        let (gates, host) = host_pair("darwin", "aarch64");
        let mut mappings_gates = HashMap::new();
        mappings_gates.insert("foo.sh".to_string(), "darwn".to_string()); // typo

        let err = scanner
            .scan_pack(&pack, &default_rules(), &[], &gates, &host, &mappings_gates)
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("darwn"), "{msg}");
        assert!(msg.contains("[mappings.gates]"), "{msg}");
    }
}
