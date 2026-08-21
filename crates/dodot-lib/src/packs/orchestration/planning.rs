//! Pack planner — the "what would we do" computation.
//!
//! Owns [`plan_pack`] (the main planner), [`plan_pack_inner`] (the
//! actual scan → preprocess → match-rules → group-by-handler →
//! to_intents pipeline), the [`PackPlan`] result type,
//! [`build_gate_table`] (`HostFacts`/`[gates]` merging), and the
//! per-pack `collect_pack_intents` API plus its diagnostic helpers
//! (`missing_target_hints`, `display_path_relative_to_home`).
//!
//! The driver in `mod.rs` calls this layer to plan one pack at a time;
//! the runner functions there then take the resulting intents and feed
//! them to the executor.

use std::path::PathBuf;

use tracing::{debug, info};

use crate::gates::{GateTable, HostFacts};
use crate::handlers;
use crate::packs::context::ExecutionContext;
use crate::packs::Pack;
use crate::rules::{self, Scanner};
use crate::Result;

// ── Built-in "up" pipeline helpers ──────────────────────────────

/// Collect handler intents for a pack **without** executing them.
///
/// Runs the scan → preprocess → match rules → group by handler →
/// to_intents pipeline and returns the generated intents. This is the
/// first half of the two-phase execution model that enables cross-pack
/// conflict detection before any mutations happen.
///
/// Uses the default preprocessor registry
/// ([`crate::preprocessing::default_registry`]).
pub fn collect_pack_intents(
    pack: &Pack,
    ctx: &ExecutionContext,
) -> Result<Vec<crate::operations::HandlerIntent>> {
    let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
    // [secret] is intentionally root-only — see SecretSection docs.
    let root_config = ctx.config_manager.root_config()?;
    let (registry, _secret_registry) = crate::preprocessing::default_registry(
        &pack_config.preprocessor,
        &root_config.secret,
        ctx.paths.as_ref(),
        ctx.command_runner.clone(),
    )?;
    collect_pack_intents_inner(pack, ctx, &pack_config, Some(&registry))
}

/// Like [`collect_pack_intents`], but accepts an explicit preprocessor
/// registry. If `None`, no preprocessing occurs.
///
/// This variant exists for testing: callers can inject a registry with
/// test preprocessors without requiring config-driven registration.
pub fn collect_pack_intents_with_preprocessors(
    pack: &Pack,
    ctx: &ExecutionContext,
    preprocessors: Option<&crate::preprocessing::PreprocessorRegistry>,
) -> Result<Vec<crate::operations::HandlerIntent>> {
    let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
    collect_pack_intents_inner(pack, ctx, &pack_config, preprocessors)
}

/// Plan for a single pack — the intents the executor will run plus
/// any soft warnings the handlers emitted during planning.
///
/// Warnings are non-fatal, human-readable strings (currently the
/// `_lib/` non-macOS skip notice from
/// `docs/proposals/macos-paths.lex` §4.2). Callers that surface
/// `PackStatusResult.warnings` should consume them; pure-execution
/// callers can ignore the field.
#[derive(Debug, Default, Clone)]
pub struct PackPlan {
    pub intents: Vec<crate::operations::HandlerIntent>,
    pub warnings: Vec<String>,
    /// Files whose handler `--no-provision` dropped before intent
    /// generation. They produce no intent and therefore no operation,
    /// so the renderers read this list to still give each one a row —
    /// otherwise a `--no-provision` run silently omits the very files
    /// the user chose to skip. Empty whenever `--no-provision` is off.
    pub provision_skipped: Vec<ProvisionSkip>,
    /// Files whose manager is not usable on this machine — absent, or
    /// impossible to probe. The dry-run renderer places their rows for
    /// the same reason as `provision_skipped`: no intent means no
    /// operation and, without a row, an absent manager reads as an
    /// empty pack.
    ///
    /// A real `up` renders through `status::status()`, which asks the
    /// same probe on its own planning pass, so this list is the
    /// dry-run half of an answer both paths compute identically. Both
    /// paths do read it for one thing: a
    /// [`ProbeFailed`](crate::provisioners::availability::Availability::ProbeFailed)
    /// entry is the only failure with no operation to carry its
    /// verdict, so `up` counts it here (ADR-0008).
    ///
    /// Ephemeral: an availability is a fact about this machine right
    /// now and is never written to the datastore.
    pub provision_unavailable: Vec<ProvisionUnavailable>,
}

/// One file dropped from a run by `--no-provision`, carrying what the
/// renderers need to place its row: the handler that would have
/// claimed it and its pack-relative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionSkip {
    pub handler: String,
    pub relative_path: String,
}

/// One file that produced no intent because its manager is not on
/// this machine.
///
/// Deliberately distinct from [`ProvisionSkip`]: `--no-provision` is
/// the user's own choice and absence is the machine's state, and the
/// two have different remedies — one is a flag the user dropped, the
/// other is a manager to install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionUnavailable {
    pub handler: String,
    pub relative_path: String,
    /// Absent (with the locations probed) or ProbeFailed (with the
    /// detail). Never `Present` — a present manager produces intents
    /// instead of a row here.
    pub availability: crate::provisioners::availability::Availability,
}

/// Like [`collect_pack_intents`], but returns both intents and any
/// soft warnings the handlers produced during planning.
///
/// Use this when surfacing per-pack warnings in user-facing output
/// (e.g. `commands::up` populating `PackStatusResult.warnings`). Pure
/// execution callers should keep using [`collect_pack_intents`].
///
/// `mode` controls the preprocessing envelope. Active runs (`dodot up`
/// with no `--dry-run`) pass [`PreprocessMode::Active`]; passive
/// callers (`dodot status`, `dodot up --dry-run`) pass
/// [`PreprocessMode::Passive`] so the pipeline reads from the
/// baseline cache instead of evaluating templates and writing
/// rendered files. See `docs/proposals/secrets.lex` §7.4.
pub fn plan_pack(
    pack: &Pack,
    ctx: &ExecutionContext,
    mode: crate::preprocessing::PreprocessMode,
) -> Result<PackPlan> {
    let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
    // [secret] is intentionally root-only — see SecretSection docs.
    let root_config = ctx.config_manager.root_config()?;
    let (registry, _secret_registry) = crate::preprocessing::default_registry(
        &pack_config.preprocessor,
        &root_config.secret,
        ctx.paths.as_ref(),
        ctx.command_runner.clone(),
    )?;
    plan_pack_inner(pack, ctx, &pack_config, Some(&registry), mode)
}

/// Resolve the gate table for a pack: built-in seed plus any
/// user-defined `[gates]` entries from config.
fn build_gate_table(pack_config: &crate::config::DodotConfig) -> Result<GateTable> {
    let mut table = GateTable::with_builtins();
    if !pack_config.gates.is_empty() {
        table.merge_user(&pack_config.gates)?;
    }
    Ok(table)
}

/// Apply pre-preprocess gate evaluation to a freshly-walked entry list.
///
/// Three gate sources are evaluated here, all *before* `preprocess_pack`
/// runs:
///
/// - **Directory-segment gates** (`_<label>/`) — already done by
///   `walk_pack`; entries arriving with `gate_failure: Some(...)` pass
///   through untouched.
/// - **Basename gates** (`<stem>._<label>.<ext>`) — parsed here,
///   passing-suffixed entries get their `relative_path` rewritten to
///   the stripped form so the preprocessor sees `aliases.sh.tmpl` (not
///   `aliases._darwin.sh.tmpl`); failing entries flip to
///   `gate_failure: Some(...)`.
/// - **`[mappings.gates]` glob → label** — globs evaluated here so a
///   mapping-gated template/secret-bearing file never reaches the
///   preprocessor either. Same flag-as-`gate_failure` flow.
///
/// Why all three at this layer: preprocessing fires render +
/// secret-provider + baseline-cache work on every preprocessor-shaped
/// file. If any gate evaluation only happened post-preprocess, a
/// gated-out template still triggers all of that for an entry the
/// user explicitly opted out of. Putting all three here keeps gates
/// honest about "predicate false ⇒ no work."
///
/// A file carrying both a filename gate AND a matching
/// `[mappings.gates]` entry is a hard error (one source of truth).
pub(crate) fn filter_pre_preprocess_gates(
    entries: Vec<crate::rules::PackEntry>,
    gates: &GateTable,
    host: &HostFacts,
    pack_name: &str,
    mappings_gates: &std::collections::HashMap<String, String>,
) -> Result<Vec<crate::rules::PackEntry>> {
    use crate::gates::{parse_basename_gate, BasenameGate};
    use crate::rules::GateFailure;

    // Shared with `match_entries` — see `gates::compile_mapping_gates`
    // for the ordering and validation contract.
    let compiled_mapping_gates = crate::gates::compile_mapping_gates(mappings_gates, pack_name)?;

    // Helper: build a GateFailure from a label + predicate, summarising
    // the host facts the predicate cares about. Shared between the
    // basename-fail and mapping-fail branches. Same compact shape as
    // `GatePredicate::describe` so the status footnote can render
    // both sides uniformly.
    let make_failure = |label: &str, pred: &crate::gates::GatePredicate| -> GateFailure {
        let host_desc: Vec<String> = pred
            .matchers
            .iter()
            .map(|(dim, _)| {
                let actual = host.get(*dim).unwrap_or("<unset>");
                format!("{}={}", dim.as_str(), actual)
            })
            .collect();
        GateFailure {
            label: label.to_string(),
            predicate: pred.describe(),
            host: host_desc.join(", "),
        }
    };

    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.gate_failure.is_some() {
            out.push(entry);
            continue;
        }

        let filename = entry
            .relative_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let basename_gate = parse_basename_gate(&filename);
        // Forward-slash-normalised path so Windows backslashes
        // don't break globs written with `/` in config and docs.
        let rel_str = crate::gates::rel_path_for_glob(&entry.relative_path);
        let mapping_match: Option<&str> = compiled_mapping_gates
            .iter()
            .find(|(pat, _)| pat.matches(&rel_str))
            .map(|(_, label)| *label);

        if let (BasenameGate::Found { .. }, Some(map_label)) = (&basename_gate, mapping_match) {
            return Err(crate::DodotError::Config(format!(
                "gate-routing conflict in pack `{pack_name}` for `{}`: \
                 file carries both a filename gate token (`._<label>`) \
                 and a `[mappings.gates]` entry (`{map_label}`). \
                 Pick one — either rename the file (drop the suffix) \
                 or remove the `[mappings.gates]` entry.",
                entry.relative_path.display()
            )));
        }

        match basename_gate {
            BasenameGate::Found { label, stripped } => {
                let pred = gates.lookup(label).ok_or_else(|| {
                    crate::DodotError::Config(format!(
                        "unknown gate label `{label}` in pack `{pack_name}`, file `{}`: \
                         label is not in the built-in seed and not defined in [gates]. \
                         Built-ins: darwin, linux, macos, arm64, aarch64, x86_64.",
                        entry.relative_path.display()
                    ))
                })?;
                if pred.matches(host) {
                    let stripped_rel = entry.relative_path.with_file_name(&stripped);
                    out.push(crate::rules::PackEntry {
                        relative_path: stripped_rel,
                        absolute_path: entry.absolute_path,
                        is_dir: entry.is_dir,
                        gate_failure: None,
                    });
                } else {
                    out.push(crate::rules::PackEntry {
                        relative_path: entry.relative_path,
                        absolute_path: entry.absolute_path,
                        is_dir: entry.is_dir,
                        gate_failure: Some(make_failure(label, pred)),
                    });
                }
            }
            BasenameGate::None => {
                if let Some(map_label) = mapping_match {
                    let pred = gates.lookup(map_label).ok_or_else(|| {
                        crate::DodotError::Config(format!(
                            "unknown gate label `{map_label}` referenced from \
                             `[mappings.gates]` in pack `{pack_name}`: label is \
                             not in the built-in seed and not defined in [gates]."
                        ))
                    })?;
                    if pred.matches(host) {
                        out.push(entry);
                    } else {
                        out.push(crate::rules::PackEntry {
                            relative_path: entry.relative_path,
                            absolute_path: entry.absolute_path,
                            is_dir: entry.is_dir,
                            gate_failure: Some(make_failure(map_label, pred)),
                        });
                    }
                } else {
                    out.push(entry);
                }
            }
        }
    }
    Ok(out)
}

fn collect_pack_intents_inner(
    pack: &Pack,
    ctx: &ExecutionContext,
    pack_config: &crate::config::DodotConfig,
    preprocessors: Option<&crate::preprocessing::PreprocessorRegistry>,
) -> Result<Vec<crate::operations::HandlerIntent>> {
    plan_pack_inner(
        pack,
        ctx,
        pack_config,
        preprocessors,
        crate::preprocessing::PreprocessMode::Active,
    )
    .map(|p| p.intents)
}

/// Same scan/preprocess/match/group/intents pipeline as
/// [`collect_pack_intents_inner`], but additionally collects
/// per-handler `warnings_for_matches` output.
///
/// Takes the pack config pre-loaded: both entrypoints load it once and
/// pass it through, so config is not re-merged per pack. `ConfigManager`
/// caches by path anyway, but threading it explicitly makes the data
/// flow obvious.
fn plan_pack_inner(
    pack: &Pack,
    ctx: &ExecutionContext,
    pack_config: &crate::config::DodotConfig,
    preprocessors: Option<&crate::preprocessing::PreprocessorRegistry>,
    mode: crate::preprocessing::PreprocessMode,
) -> Result<PackPlan> {
    let rules = crate::config::mappings_to_rules(&pack_config.mappings);
    let gates = build_gate_table(pack_config)?;
    let host = ctx.host_facts.as_ref();

    // [pack] os gate — short-circuit inactive packs. Without this,
    // intent collection still runs for packs the host doesn't deploy,
    // which can hit cross-pack conflict detection or trigger
    // preprocessor side-effects (template render, secret-provider
    // calls) that the user explicitly opted out of via `[pack] os`.
    if !crate::gates::pack_os_active(&pack_config.pack.os, host) {
        debug!(
            pack = %pack.name,
            allowed = ?pack_config.pack.os,
            current_os = %host.os,
            "pack inactive on this OS, returning empty plan"
        );
        return Ok(PackPlan {
            intents: Vec::new(),
            warnings: Vec::new(),
            provision_skipped: Vec::new(),
            provision_unavailable: Vec::new(),
        });
    }

    // Phase 1: Walk pack directory. The walk handles directory-segment
    // gates (`_<label>/`) — passing gates expand transparently, failing
    // gates surface as PackEntry { gate_failure: Some(...) }.
    let scanner = Scanner::new(ctx.fs.as_ref());
    let entries = scanner.walk_pack(&pack.path, &pack_config.pack.ignore, &gates, host)?;
    debug!(pack = %pack.name, entries = entries.len(), "walked pack directory");

    // Phase 1.5: Apply the remaining gate sources before preprocessing
    // — see `filter_pre_preprocess_gates` for why they belong here.
    // match_entries re-evaluates these gates so a failure also surfaces
    // as a `gate`-handler match.
    let entries = filter_pre_preprocess_gates(
        entries,
        &gates,
        host,
        &pack.name,
        &pack_config.mappings.gates,
    )?;

    // Phase 2: Preprocessing
    let preprocess_result = if let Some(registry) = preprocessors {
        if !registry.is_empty() && pack_config.preprocessor.enabled {
            crate::preprocessing::pipeline::preprocess_pack(
                entries,
                registry,
                pack,
                ctx.fs.as_ref(),
                ctx.datastore.as_ref(),
                ctx.paths.as_ref(),
                mode,
                ctx.force,
            )?
        } else {
            crate::preprocessing::pipeline::PreprocessResult::passthrough(entries)
        }
    } else {
        crate::preprocessing::pipeline::PreprocessResult::passthrough(entries)
    };

    // Phase 3: Merge and match rules. Reuse the gate table + host
    // facts from phase 1 so basename/dir gates see the same view.
    // (Failed basename gates were already converted to gate-handler
    // matches in phase 1.5; match_entries sees them as gate_failure
    // entries and re-emits them.)
    let all_entries = preprocess_result.merged_entries();
    let mut matches = scanner.match_entries(
        &all_entries,
        &rules,
        &pack.name,
        &gates,
        host,
        &pack_config.mappings.gates,
    )?;
    debug!(pack = %pack.name, files = matches.len(), "matched rules");

    // Propagate preprocessor source info and in-memory rendered
    // bytes onto each match. Handlers that hash rendered content
    // for sentinel construction (`install`, `homebrew`) read the
    // bytes from `m.rendered_bytes` first, falling back to disk
    // for non-template files. That decoupling is the structural
    // enabler for §7.4 Passive mode where rendered files are
    // intentionally not on disk. See issue #121.
    for m in &mut matches {
        if let Some(source) = preprocess_result.source_map.get(&m.absolute_path) {
            m.preprocessor_source = Some(source.clone());
        }
        if let Some(bytes) = preprocess_result.rendered_bytes.get(&m.absolute_path) {
            m.rendered_bytes = Some(bytes.clone());
        }
    }

    // Phase 4: Group by handler
    let groups = rules::group_by_handler(&matches);

    // Build handler registry (drives the phase-based execution order).
    let registry = handlers::create_registry(ctx.fs.as_ref());
    let order = rules::handler_execution_order(&groups, &registry);
    debug!(pack = %pack.name, handlers = ?order, "handler execution order");

    // Generate intents from each handler
    let mut all_intents = Vec::new();
    let mut all_warnings = Vec::new();
    let mut provision_skipped: Vec<ProvisionSkip> = Vec::new();
    let mut provision_unavailable: Vec<ProvisionUnavailable> = Vec::new();

    // Surface preserved-divergent-file warnings from the preprocessing
    // pipeline. These are the §6.4 "deployed file edited" cases: dodot
    // refused to overwrite the user's edit and held the previous render
    // in place. The user resolves them via `dodot transform check`
    // (auto-merge through the clean filter) or `dodot up --force`
    // (overwrite).
    for skipped in &preprocess_result.skipped {
        let display_path = display_path_relative_to_home(&skipped.deployed_path, ctx);
        let detail = match skipped.state {
            crate::preprocessing::divergence::DivergenceState::OutputChanged => {
                "deployed file was edited since the last `dodot up`"
            }
            crate::preprocessing::divergence::DivergenceState::BothChanged => {
                "both the source template and the deployed file were edited since the last `dodot up`"
            }
            _ => "deployed file diverges from the cached baseline",
        };
        let warning = format!(
            "preserved {} ({}). Run `dodot transform check` to reconcile, or re-run with --force to overwrite.",
            display_path, detail,
        );
        tracing::warn!(pack = %pack.name, file = %skipped.virtual_relative.display(), "{warning}");
        all_warnings.push(warning);
    }
    for handler_name in &order {
        let handler = match registry.get(handler_name.as_str()) {
            Some(h) => h,
            None => {
                debug!(pack = %pack.name, handler = %handler_name, "skipping unknown handler");
                continue;
            }
        };

        if ctx.no_provision && handler.category() == handlers::HandlerCategory::CodeExecution {
            debug!(pack = %pack.name, handler = %handler_name, "skipping code-execution handler (--no-provision)");
            // Record what was dropped. No intent means no operation
            // and, without this, no row at all — the user would be
            // told nothing about the files they asked to skip.
            if let Some(handler_matches) = groups.get(handler_name) {
                for m in handler_matches {
                    if m.is_dir {
                        continue;
                    }
                    provision_skipped.push(ProvisionSkip {
                        handler: handler_name.clone(),
                        relative_path: m.relative_path.to_string_lossy().into_owned(),
                    });
                }
            }
            continue;
        }

        // Is the manager there? Asked once per provisioning handler
        // the pack actually matched files for — a machine with no
        // `Brewfile` never probes for brew — and asked before any
        // intent exists, because an absent manager must produce no
        // intent, no receipt, and no error.
        //
        // The answer comes from the one shared module `status` also
        // reads, so the two agree by construction. `install` is not
        // located by dodot and always answers present; see
        // `provisioners::availability`.
        //
        // A present answer also names *which* executable answered,
        // and that path is kept: the run has to spawn the brew the
        // probe found, not whatever `PATH` resolves later. See
        // `located_at` at its use below.
        let mut located_at: Option<PathBuf> = None;
        if crate::provisioners::descriptor_for(handler_name).is_some() {
            let availability = crate::provisioners::availability::probe(
                ctx.fs.as_ref(),
                ctx.provision_host.as_ref(),
                handler_name,
            );
            if let crate::provisioners::availability::Availability::Present { at } = &availability {
                located_at = at.clone();
            }
            if !availability.is_present() {
                debug!(
                    pack = %pack.name,
                    handler = %handler_name,
                    ?availability,
                    "skipping provisioning handler — manager unusable on this host"
                );
                if let Some(handler_matches) = groups.get(handler_name) {
                    for m in handler_matches {
                        if m.is_dir {
                            continue;
                        }
                        provision_unavailable.push(ProvisionUnavailable {
                            handler: handler_name.clone(),
                            relative_path: m.relative_path.to_string_lossy().into_owned(),
                            availability: availability.clone(),
                        });
                    }
                }
                continue;
            }
        }

        if let Some(handler_matches) = groups.get(handler_name) {
            let mut intents = handler.to_intents(
                handler_matches,
                &pack.config,
                ctx.paths.as_ref(),
                ctx.fs.as_ref(),
            )?;
            // Run the executable the probe found, not the name.
            //
            // `command_for` names its program the way a user would
            // (`brew`, `nix`), which leaves the OS to resolve it
            // through `PATH` at spawn time — a second, different
            // question from the one the probe just answered. A brew
            // sitting at `/opt/homebrew/bin/brew` on a host whose
            // `PATH` omits it would pass the probe and then fail to
            // spawn, and the probe's whole promise is that a run
            // dodot planned is a run dodot can make. Substituting the
            // located path here keeps `command_for` a pure function
            // of the manifest path and leaves the arguments — and so
            // the manifest positions declared in
            // `provisioners::PROVISIONERS` — untouched.
            //
            // `None` for `install`, whose interpreter is a `PATH`
            // lookup by design (ADR-0007), and for any handler dodot
            // does not locate.
            //
            // Substituted only when the path can be *named* in the
            // `String` an intent's executable is. A lossy conversion
            // would be the worst of the three outcomes: the probe
            // read `$HOMEBREW_PREFIX` as bytes precisely so a
            // non-UTF-8 prefix keeps its candidate, and replacing
            // those bytes with U+FFFD here would hand the spawn a
            // path that names nothing — dodot would find brew and
            // then fail to run it, reporting "not found" about a file
            // it had just stat'd. Leaving the handler's own name
            // instead puts the row back where every provisioning row
            // was before the probe existed: the OS resolves it
            // through `PATH`, which is what `install` does by design.
            // Carrying the bytes through to the spawn means an
            // OS-native executable type across `HandlerIntent`,
            // `Operation`, `CommandSpec`, and every `CommandRunner` —
            // an epic-wide change, not this handler's to make.
            if let Some(at) = &located_at {
                match at.to_str() {
                    Some(program) => {
                        for intent in &mut intents {
                            if let crate::operations::HandlerIntent::Run { executable, .. } = intent
                            {
                                *executable = program.to_string();
                            }
                        }
                    }
                    None => {
                        let warning = format!(
                            "{handler_name} was found at {}, a path dodot cannot name exactly. \
                             Running `{handler_name}` as your shell would resolve it instead.",
                            at.display()
                        );
                        tracing::warn!(pack = %pack.name, handler = %handler_name, "{warning}");
                        all_warnings.push(warning);
                    }
                }
            }
            debug!(
                pack = %pack.name,
                handler = %handler_name,
                intents = intents.len(),
                "generated intents"
            );
            all_intents.extend(intents);

            let warnings =
                handler.warnings_for_matches(handler_matches, &pack.config, ctx.paths.as_ref());
            for w in &warnings {
                tracing::warn!(pack = %pack.name, handler = %handler_name, "{w}");
            }
            all_warnings.extend(warnings);
        }
    }

    // Missing-target hints — macOS only.
    //
    // For each Link intent that lands under `app_support_dir`, check
    // whether the immediate child folder exists on disk. If not, the
    // user is about to deploy GUI-app config to a directory the app
    // hasn't created yet — usually because the app isn't installed.
    // Surface a soft hint, optionally enriched with a matching brew
    // cask token. Resolver/intent state is unaffected.
    //
    // On Linux `app_support_dir` collapses to `xdg_config_home`, so
    // this check would fire for *every* `~/.config/<X>/` deploy —
    // not what we want. Gate on macOS strictly.
    if cfg!(target_os = "macos") {
        all_warnings.extend(missing_target_hints(&all_intents, ctx));
    }

    info!(
        pack = %pack.name,
        intents = all_intents.len(),
        warnings = all_warnings.len(),
        "collected intents"
    );
    Ok(PackPlan {
        intents: all_intents,
        warnings: all_warnings,
        provision_skipped,
        provision_unavailable,
    })
}

/// Render an absolute path with `$HOME` collapsed to `~` for human
/// display. Falls back to the absolute form when the path is outside
/// the home tree.
fn display_path_relative_to_home(path: &std::path::Path, ctx: &ExecutionContext) -> String {
    let home = ctx.paths.home_dir();
    match path.strip_prefix(home) {
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Probe each `Link` intent that targets `app_support_dir/<X>/...` and
/// emit a soft hint when the `<X>/` folder is missing on disk.
///
/// macOS-only — caller checks `cfg!(target_os = "macos")` first to
/// avoid firing on Linux where every XDG-routed entry would otherwise
/// hit this branch.
fn missing_target_hints(
    intents: &[crate::operations::HandlerIntent],
    ctx: &ExecutionContext,
) -> Vec<String> {
    use std::collections::BTreeSet;
    let app_support = ctx.paths.app_support_dir();
    if app_support == ctx.paths.xdg_config_home() {
        // `app_uses_library = false` collapsed the app-support root
        // onto XDG; same Linux-style suppression applies.
        return Vec::new();
    }

    // Distinct `<X>` folders referenced by intents — one warning per
    // missing folder, regardless of how many files target it.
    let mut needed: BTreeSet<String> = BTreeSet::new();
    for intent in intents {
        if let crate::operations::HandlerIntent::Link { user_path, .. } = intent {
            if let Ok(rel) = user_path.strip_prefix(app_support) {
                if let Some(first) = rel.components().find_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    _ => None,
                }) {
                    needed.insert(first);
                }
            }
        }
    }
    if needed.is_empty() {
        return Vec::new();
    }

    let mut missing: Vec<String> = Vec::new();
    for folder in &needed {
        let target = app_support.join(folder);
        if !ctx.fs.exists(&target) {
            missing.push(folder.clone());
        }
    }
    if missing.is_empty() {
        return Vec::new();
    }

    // Brew enrichment: try to associate each missing folder with an
    // *installed* cask token. Cache-only mode keeps the planner fast:
    // a stale or missing cache entry silently degrades to the
    // unenriched message rather than spawning a `brew info` subprocess
    // per installed cask. The on-demand `dodot probe app` subcommand
    // populates the cache; this hint just consumes it.
    let cache_dir = ctx.paths.probes_brew_cache_dir();
    let now = crate::probe::brew::now_secs_unix();
    let matches = crate::probe::brew::match_folders_to_installed_casks(
        &missing,
        ctx.command_runner.as_ref(),
        &cache_dir,
        now,
        ctx.fs.as_ref(),
        /*cache_only=*/ true,
    );

    missing
        .into_iter()
        .map(|folder| match matches.folder_to_token.get(&folder) {
            // The cask IS installed (we got the token from `brew list`)
            // but the folder is empty — usually the user pre-deployed
            // dotfiles before launching the app for the first time.
            Some(token) => format!(
                "cask `{token}` is installed but `{folder}/` is missing — \
                 entries will deploy, but the app may not have created its \
                 config directory yet (try launching it once)"
            ),
            None => format!(
                "target directory `{}/{folder}` doesn't exist yet — entries will \
                 deploy but no matching installed app appears to provide it",
                app_support.display()
            ),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]

    use std::sync::Arc;

    use super::super::test_support::{make_context, MockCommandRunner, TestUpCommand};
    use super::super::{
        collect_pack_intents, execute, execute_intents, prepare_packs, run_handler_pipeline,
    };
    use super::{collect_pack_intents_with_preprocessors, plan_pack};
    use crate::config::ConfigManager;
    use crate::datastore::CommandRunner;
    use crate::datastore::FilesystemDataStore;
    use crate::fs::Fs;
    use crate::packs::Pack;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

    // ── --no-provision bookkeeping ─────────────────────────────

    /// The planner is the only place that knows a code-execution
    /// handler was dropped: the drop happens before intent generation,
    /// so nothing downstream can infer it from the intents. Both
    /// renderers read this list to give the dropped files a row.
    #[test]
    fn no_provision_records_what_it_dropped() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("install.sh", "#!/bin/sh\necho setup")
            .done()
            .build();

        let ctx = make_context(&env); // no_provision = true
        let pack = Pack::new(
            "vim".into(),
            env.dotfiles_root.join("vim"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("vim"))
                .unwrap()
                .to_handler_config(),
        );

        let plan = plan_pack(&pack, &ctx, crate::preprocessing::PreprocessMode::Passive).unwrap();

        assert_eq!(
            plan.provision_skipped,
            vec![super::ProvisionSkip {
                handler: "install".into(),
                relative_path: "install.sh".into(),
            }],
        );
        assert!(
            plan.intents
                .iter()
                .any(|i| matches!(i, crate::operations::HandlerIntent::Link { .. })),
            "configuration handlers must still plan normally"
        );
    }

    #[test]
    fn provisioning_runs_leave_the_skip_list_empty() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "#!/bin/sh\necho setup")
            .done()
            .build();

        let mut ctx = make_context(&env);
        ctx.no_provision = false;
        let pack = Pack::new(
            "vim".into(),
            env.dotfiles_root.join("vim"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("vim"))
                .unwrap()
                .to_handler_config(),
        );

        let plan = plan_pack(&pack, &ctx, crate::preprocessing::PreprocessMode::Passive).unwrap();

        assert!(
            plan.provision_skipped.is_empty(),
            "nothing was skipped: {:?}",
            plan.provision_skipped
        );
    }

    // ── Preprocessing integration tests ────────────────────────

    #[test]
    fn preprocessing_identity_file_deploys_via_symlink_handler() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "host = localhost")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        assert_eq!(intents.len(), 1, "intents: {intents:?}");

        match &intents[0] {
            crate::operations::HandlerIntent::Link {
                pack: p,
                handler,
                source,
                user_path,
            } => {
                assert_eq!(p, "app");
                assert_eq!(handler, "symlink");
                assert!(
                    source.to_string_lossy().contains("preprocessed"),
                    "source should be in preprocessed dir: {}",
                    source.display()
                );
                let user_str = user_path.to_string_lossy();
                assert!(
                    !user_str.contains("identity"),
                    "user_path should not have .identity: {user_str}"
                );
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn preprocessing_mixed_pack_deploys_both() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "preprocessed content")
            .file("plain.txt", "regular content")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        assert_eq!(intents.len(), 2, "intents: {intents:?}");

        let intent_sources: Vec<String> = intents
            .iter()
            .filter_map(|i| match i {
                crate::operations::HandlerIntent::Link { source, .. } => {
                    Some(source.to_string_lossy().to_string())
                }
                _ => None,
            })
            .collect();

        let has_preprocessed = intent_sources.iter().any(|s| s.contains("preprocessed"));
        let has_regular = intent_sources
            .iter()
            .any(|s| s.contains("dotfiles/app/plain.txt"));
        assert!(
            has_preprocessed,
            "should have a preprocessed source: {intent_sources:?}"
        );
        assert!(
            has_regular,
            "should have a regular source: {intent_sources:?}"
        );
    }

    #[test]
    fn preprocessing_collision_detected() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "preprocessed")
            .file("config.toml", "regular")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let err =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap_err();
        assert!(
            matches!(err, crate::DodotError::PreprocessorCollision { .. }),
            "expected PreprocessorCollision, got: {err}"
        );
    }

    #[test]
    fn preprocessing_disabled_via_config_treats_files_as_regular() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "content")
            .done()
            .build();

        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                b"[preprocessor]\nenabled = false\n",
            )
            .unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        assert_eq!(intents.len(), 1);
        match &intents[0] {
            crate::operations::HandlerIntent::Link { user_path, .. } => {
                let user_str = user_path.to_string_lossy();
                assert!(
                    user_str.contains("identity"),
                    "with preprocessing disabled, file should keep .identity extension: {user_str}"
                );
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn preprocessing_no_registry_works_like_before() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "vim".into(),
            env.dotfiles_root.join("vim"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("vim"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents_with_preprocessors(&pack, &ctx, None).unwrap();

        assert_eq!(intents.len(), 1);
        match &intents[0] {
            crate::operations::HandlerIntent::Link { source, .. } => {
                assert!(
                    source.to_string_lossy().contains("vim/vimrc"),
                    "source should be the pack file: {}",
                    source.display()
                );
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn preprocessing_end_to_end_deploy_and_verify_content() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "host = localhost\nport = 5432")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        let user_path = match &intents[0] {
            crate::operations::HandlerIntent::Link { user_path, .. } => user_path.clone(),
            other => panic!("expected Link intent, got: {other:?}"),
        };

        let results = execute_intents(intents, &ctx).unwrap();

        assert!(
            results.iter().all(|r| r.success),
            "all operations should succeed: {results:?}"
        );

        assert!(
            ctx.fs.exists(&user_path),
            "user file should exist at: {}",
            user_path.display()
        );
        assert!(
            ctx.fs.is_symlink(&user_path),
            "user file should be a symlink"
        );

        let content = ctx.fs.read_to_string(&user_path).unwrap();
        assert_eq!(content, "host = localhost\nport = 5432");
    }

    #[test]
    fn preprocessing_error_propagates_through_pipeline() {
        // Expansion errors should propagate through the pipeline.
        // We test this at the pipeline level (not orchestration) since
        // the scanner won't see a file that doesn't exist. The pipeline
        // tests in pipeline.rs cover this case directly. Here we verify
        // that a valid preprocessor file that triggers an error during
        // a lower-level operation still propagates correctly.
        //
        // Use the unarchive preprocessor with a corrupted archive.
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("bad.tar.gz", "this is not valid gzip data at all")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "tools".into(),
            env.dotfiles_root.join("tools"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("tools"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::unarchive::UnarchivePreprocessor::new(),
        ));

        let err =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap_err();
        assert!(
            matches!(err, crate::DodotError::PreprocessorError { .. }),
            "expected PreprocessorError, got: {err}"
        );
    }

    #[test]
    fn preprocessing_multiple_types_in_registry() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "identity content")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));
        registry.register(Box::new(
            crate::preprocessing::unarchive::UnarchivePreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        assert_eq!(intents.len(), 1);
        match &intents[0] {
            crate::operations::HandlerIntent::Link { source, .. } => {
                assert!(source.to_string_lossy().contains("preprocessed"));
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn collect_pack_intents_uses_default_registry() {
        // The normal `collect_pack_intents` entrypoint should wire the
        // default preprocessor registry (not pass `None`). We verify
        // this by putting a `.tar.gz` file in a pack — the default
        // registry contains `UnarchivePreprocessor`, so the archive
        // should be expanded rather than passed through.
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let env = TempEnvironment::builder()
            .pack("tools")
            .file("placeholder", "")
            .done()
            .build();

        let archive_path = env.dotfiles_root.join("tools/payload.tar.gz");
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(enc);
        let content = b"#!/bin/sh\necho hi";
        let mut header = tar::Header::new_gnu();
        header.set_path("mytool").unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, &content[..]).unwrap();
        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "tools".into(),
            env.dotfiles_root.join("tools"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("tools"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();

        let has_expanded_source = intents.iter().any(|i| match i {
            crate::operations::HandlerIntent::Link { source, .. } => {
                source.to_string_lossy().contains("preprocessed")
                    && source.to_string_lossy().contains("mytool")
            }
            _ => false,
        });
        assert!(
        has_expanded_source,
        "production collect_pack_intents should expand .tar.gz via the default registry. Intents: {intents:?}"
    );
    }

    // ── Template preprocessor integration tests ─────────────────

    #[test]
    fn template_deploys_rendered_content_via_symlink_handler() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file(
                "config.toml.tmpl",
                "name = \"{{ name }}\"\nos = \"{{ dodot.os }}\"",
            )
            .config("[preprocessor.template.vars]\nname = \"Alice\"\n")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        let user_path = match &intents[0] {
            crate::operations::HandlerIntent::Link { user_path, .. } => user_path.clone(),
            other => panic!("expected Link intent, got: {other:?}"),
        };

        let results = execute_intents(intents, &ctx).unwrap();
        assert!(
            results.iter().all(|r| r.success),
            "expected success: {results:?}"
        );

        let content = ctx.fs.read_to_string(&user_path).unwrap();
        let expected_os = std::env::consts::OS;
        assert_eq!(content, format!("name = \"Alice\"\nos = \"{expected_os}\""));
    }

    #[test]
    fn template_with_shell_handler_sources_rendered_content() {
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("aliases.sh.tmpl", "alias hello='echo {{ greeting }}'")
            .config("[preprocessor.template.vars]\ngreeting = \"world\"\n")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "tools".into(),
            env.dotfiles_root.join("tools"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("tools"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        assert_eq!(intents.len(), 1);

        match &intents[0] {
            crate::operations::HandlerIntent::Stage {
                handler, source, ..
            } => {
                assert_eq!(handler, "shell", "shell handler should own this");
                let content = ctx.fs.read_to_string(source).unwrap();
                assert_eq!(content, "alias hello='echo world'");
            }
            other => panic!("expected Stage intent, got: {other:?}"),
        }
    }

    #[test]
    fn template_respects_per_pack_var_overrides() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("greeting.tmpl", "hello {{ name }}")
            .config("[preprocessor.template.vars]\nname = \"Bob\"\n")
            .done()
            .build();

        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                b"[preprocessor.template.vars]\nname = \"Alice\"\n",
            )
            .unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        match &intents[0] {
            crate::operations::HandlerIntent::Link { source, .. } => {
                let content = ctx.fs.read_to_string(source).unwrap();
                assert_eq!(content, "hello Bob", "pack-level override should win");
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn template_disabled_via_config_treats_files_as_regular() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = \"{{ name }}\"")
            .done()
            .build();

        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                b"[preprocessor]\nenabled = false\n",
            )
            .unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        assert_eq!(intents.len(), 1);
        match &intents[0] {
            crate::operations::HandlerIntent::Link {
                source, user_path, ..
            } => {
                assert!(
                    source.to_string_lossy().ends_with("config.toml.tmpl"),
                    "source: {}",
                    source.display()
                );
                assert!(
                    user_path.to_string_lossy().contains(".tmpl"),
                    "user_path should keep .tmpl extension: {}",
                    user_path.display()
                );
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn template_render_error_surfaces_with_source_path() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bad.tmpl", "value = \"{{ undefined_var }}\"")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let err = collect_pack_intents(&pack, &ctx).unwrap_err();
        match err {
            crate::DodotError::TemplateRender { source_file, .. } => {
                assert!(
                    source_file.ends_with("bad.tmpl"),
                    "source_file: {}",
                    source_file.display()
                );
            }
            other => panic!("expected TemplateRender, got: {other:?}"),
        }
    }

    #[test]
    fn template_reserved_var_fails_fast() {
        // A user tries to define `dodot` as a variable — construction
        // of the preprocessor should fail before any rendering happens.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("file.txt", "x")
            .done()
            .build();

        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                b"[preprocessor.template.vars]\ndodot = \"pwn\"\n",
            )
            .unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let err = collect_pack_intents(&pack, &ctx).unwrap_err();
        assert!(
            matches!(err, crate::DodotError::TemplateReservedVar { ref name } if name == "dodot"),
            "got: {err}"
        );
    }

    #[test]
    fn template_with_install_handler_sentinel_reflects_rendered_content() {
        // install.sh.tmpl should render, and the sentinel should be
        // based on the rendered content (so vars changes re-run the
        // script). Verify by checking the sentinel name includes the
        // hash of the rendered content, not the template source.
        let env = TempEnvironment::builder()
            .pack("setup")
            .file(
                "install.sh.tmpl",
                "#!/bin/sh\necho \"installing on {{ dodot.os }}\"",
            )
            .done()
            .build();

        let mut ctx = make_context(&env);
        ctx.no_provision = false; // actually run install this time

        let pack = Pack::new(
            "setup".into(),
            env.dotfiles_root.join("setup"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("setup"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        let (sentinel, rendered_path) = match &intents[0] {
            crate::operations::HandlerIntent::Run {
                sentinel,
                arguments,
                ..
            } => (
                sentinel.clone(),
                std::path::PathBuf::from(
                    crate::provisioners::manifest_argument("install", arguments)
                        .expect("the install descriptor names the script argument"),
                ),
            ),
            other => panic!("expected Run intent, got: {other:?}"),
        };

        // Sentinel is "install.sh-{checksum}" where checksum is the
        // SHA-256 of the *rendered* script in the datastore.
        assert!(sentinel.starts_with("install.sh-"));

        let content = ctx.fs.read_to_string(&rendered_path).unwrap();
        assert!(
            content.contains(std::env::consts::OS),
            "rendered content should have OS substituted: {content}"
        );
    }

    #[test]
    fn plan_pack_surfaces_divergence_warnings() {
        // End-to-end: a template-deployed file gets edited by the user,
        // then `plan_pack` runs again. The pipeline preserves the edit
        // and `PackPlan.warnings` carries a human-readable warning that
        // mentions the deployed path, the resolution paths, and `--force`.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        // First run: clean deploy, no warnings about preserved files.
        let first = plan_pack(&pack, &ctx, crate::preprocessing::PreprocessMode::Active).unwrap();
        assert!(
            first.warnings.iter().all(|w| !w.contains("preserved")),
            "first deploy must not produce a preservation warning: {:?}",
            first.warnings
        );

        // User edits the deployed file.
        let deployed = env
            .paths
            .handler_data_dir("app", "preprocessed")
            .join("config.toml");
        env.fs.write_file(&deployed, b"name = USER EDITED").unwrap();

        // Second run: warning surfaces, with the documented resolution
        // hints — `transform check` and `--force`.
        let second = plan_pack(&pack, &ctx, crate::preprocessing::PreprocessMode::Active).unwrap();
        let preserved: Vec<&String> = second
            .warnings
            .iter()
            .filter(|w| w.contains("preserved"))
            .collect();
        assert_eq!(
            preserved.len(),
            1,
            "expected one preservation warning, got: {:?}",
            second.warnings
        );
        let w = preserved[0];
        assert!(
            w.contains("config.toml"),
            "warning should name the file: {w}"
        );
        assert!(
            w.contains("transform check"),
            "warning should mention transform check: {w}"
        );
        assert!(w.contains("--force"), "warning should mention --force: {w}");
        // The user's edit must still be on disk.
        assert_eq!(
            env.fs.read_to_string(&deployed).unwrap(),
            "name = USER EDITED"
        );
    }

    #[test]
    fn plan_pack_force_overwrites_and_skips_warning() {
        // With ctx.force=true (the `--force` CLI flag), the guard is
        // bypassed: the deployed file gets re-rendered, no warning is
        // emitted. Documented escape hatch for env-var rotations and
        // similar out-of-band changes.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let mut ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let _ = plan_pack(&pack, &ctx, crate::preprocessing::PreprocessMode::Active).unwrap();
        let deployed = env
            .paths
            .handler_data_dir("app", "preprocessed")
            .join("config.toml");
        env.fs.write_file(&deployed, b"name = USER EDITED").unwrap();

        ctx.force = true;
        let plan = plan_pack(&pack, &ctx, crate::preprocessing::PreprocessMode::Active).unwrap();
        assert!(
            plan.warnings.iter().all(|w| !w.contains("preserved")),
            "force=true must not emit preservation warnings: {:?}",
            plan.warnings
        );
        assert_eq!(
            env.fs.read_to_string(&deployed).unwrap(),
            "name = original",
            "force must overwrite the user's edit with the rendered content"
        );
    }
}
