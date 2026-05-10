//! Pack scanner: walks pack directories and matches entries against rules.
//!
//! [`Scanner`] is the unit that turns "this directory" into "these matches."
//! Pattern compilation lives in [`crate::rules::pattern`]; types live in
//! [`crate::rules::types`]. This file owns directory walking, gate
//! evaluation, and the per-entry rule application loop.

use std::collections::HashMap;
use std::path::Path;

use crate::fs::Fs;
use crate::gates::{parse_basename_gate, BasenameGate, GateTable, HostFacts};
use crate::handlers::HANDLER_GATE;
use crate::packs::Pack;
use crate::rules::pattern::{compile_rules, match_file, CompiledRule};
use crate::rules::{GateFailure, PackEntry, Rule, RuleMatch};
use crate::{DodotError, Result};

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

        // Compile + sort + validate `[mappings.gates]` globs via the
        // shared helper so the up-planning path
        // (`filter_pre_preprocess_gates`) and this matcher can never
        // disagree about iteration order, validation, or first-match
        // semantics. See `gates::compile_mapping_gates`.
        let compiled_mapping_gates =
            crate::gates::compile_mapping_gates(mappings_gates, pack_name)?;

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
            // Forward-slash-normalised path so Windows backslashes
            // don't break globs written with `/` in config and docs.
            let rel_str = crate::gates::rel_path_for_glob(&entry.relative_path);
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

#[cfg(test)]
mod tests;
