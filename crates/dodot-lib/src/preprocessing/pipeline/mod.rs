//! Preprocessing pipeline — partitions, expands, and merges entries.
//!
//! This module contains the core pipeline function that runs between
//! directory walking and rule matching. It identifies preprocessor files,
//! expands them, writes results to the datastore, checks for collisions,
//! and produces virtual entries for the handler pipeline.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use tracing::{debug, info};

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::packs::Pack;
use crate::paths::Pather;
use crate::preprocessing::baseline::{cache_filename_for, hex_encode_32, hex_sha256, Baseline};
use crate::preprocessing::divergence::DivergenceState;
use crate::preprocessing::PreprocessorRegistry;
use crate::rules::PackEntry;
use crate::{DodotError, Result};

/// Execution envelope for the preprocessing pipeline.
///
/// `secrets.lex` §7.4 ("Auth Fatigue and Passive Commands") draws a
/// hard line between two envelopes:
///
/// - **Active** (`dodot up`): evaluates templates, batches `secret()`
///   calls per provider, prompts for auth once per run, writes
///   rendered files and baselines to disk.
/// - **Passive** (`dodot status`, `dodot up --dry-run`): MUST NOT
///   evaluate templates. Drift detection runs entirely off the
///   baseline cache. No provider calls. No datastore writes. No
///   baseline writes.
///
/// This enum is the single boolean the pipeline gates on. See issue
/// #121.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreprocessMode {
    /// Run preprocessors, write rendered outputs to the datastore,
    /// write baselines to the cache. The `dodot up` path.
    Active,
    /// Read everything from the baseline cache. Skip preprocessor
    /// expansion (no provider calls), skip datastore writes, skip
    /// baseline writes. For preprocessor entries with no baseline
    /// yet, surface a passthrough placeholder so callers can render
    /// "unknown — run `dodot up` first" without falling through to
    /// template evaluation.
    Passive,
}

/// Validate that a preprocessor-produced path is safe to materialise in
/// the datastore: relative, no root/prefix/parent-dir components, and
/// not effectively empty.
///
/// Malicious or malformed preprocessor output (tar-slip, absolute paths,
/// `..` segments) can escape the pack namespace and overwrite arbitrary
/// files. Empty paths (or paths made up only of `.` components) are
/// rejected because they would silently fail at the datastore layer with
/// an opaque error — here we produce a clean diagnostic naming the
/// preprocessor and source file.
fn validate_safe_relative_path(path: &Path, preprocessor: &str, source_file: &Path) -> Result<()> {
    let mut has_normal = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_normal = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(DodotError::PreprocessorError {
                    preprocessor: preprocessor.into(),
                    source_file: source_file.to_path_buf(),
                    message: format!(
                        "unsafe path in preprocessor output: {} (absolute or contains `..`)",
                        path.display()
                    ),
                });
            }
        }
    }
    if !has_normal {
        return Err(DodotError::PreprocessorError {
            preprocessor: preprocessor.into(),
            source_file: source_file.to_path_buf(),
            message: format!(
                "preprocessor produced an empty output path (\"{}\"). This usually means a file like \
                 `.tmpl` or `.identity` has no stem after stripping the preprocessor extension — \
                 rename the source file so that it has a non-empty name after stripping.",
                path.display()
            ),
        });
    }
    Ok(())
}

/// Normalise a validated relative path by dropping `CurDir` components,
/// so that `./foo` and `foo` are treated as the same virtual path for
/// collision detection. Only call after [`validate_safe_relative_path`].
fn normalize_relative(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        if let Component::Normal(n) = component {
            out.push(n);
        }
    }
    out
}

/// The result of preprocessing a pack's file entries.
#[derive(Debug)]
pub struct PreprocessResult {
    /// Entries that were NOT preprocessed (pass through unchanged).
    pub regular_entries: Vec<PackEntry>,
    /// Virtual entries created by preprocessing (point to datastore files).
    pub virtual_entries: Vec<PackEntry>,
    /// Maps virtual entry absolute_path → original source path in pack.
    pub source_map: HashMap<PathBuf, PathBuf>,
    /// Maps virtual entry absolute_path → in-memory rendered bytes.
    /// Populated for every virtual entry in Active mode, and in
    /// Passive mode for every entry that has a cached baseline to
    /// source the bytes from. Handlers that need the rendered content
    /// for sentinel hashing (`install`, `homebrew`) consult this map
    /// first and fall back to disk read for non-template files.
    /// Without this, Passive callers — where the rendered file isn't
    /// on disk — couldn't produce correct sentinels for templated
    /// install scripts or Brewfiles.
    ///
    /// A Passive entry with no baseline has no bytes here and is
    /// listed in [`Self::unrendered`] instead. An entry whose baseline
    /// is superseded appears in both: the bytes it carries are the
    /// previous render, which is what is deployed today.
    pub rendered_bytes: HashMap<PathBuf, Arc<[u8]>>,
    /// Virtual entries whose *current* source has not been rendered.
    /// Keyed by the virtual entry's absolute (datastore) path, the
    /// same key `rendered_bytes` and `source_map` use. Passive mode
    /// produces them two ways:
    ///
    /// - **Never rendered.** No baseline exists, and evaluating the
    ///   template here to get one would be the §7.4 violation Passive
    ///   exists to avoid. The entry carries no `rendered_bytes`.
    /// - **Superseded baseline.** A baseline exists but was rendered
    ///   from source bytes or a rendering context that have since
    ///   changed, so what it holds is the previous render rather than
    ///   what the next `dodot up` will produce. Its bytes are still in
    ///   `rendered_bytes`, because they describe what is deployed
    ///   right now — which is the question `status` asks.
    ///
    /// Always empty in Active mode, which renders every entry it
    /// surfaces.
    ///
    /// A handler that derives its claims from file *content* —
    /// `externals`, whose targets live inside `externals.toml` — emits
    /// either nothing (never rendered) or the previous render's claims
    /// (superseded) for these entries, and neither is this pack's
    /// current claim set. Callers that must know that set before
    /// mutating anything read this list to tell "no claims" apart from
    /// "claims dodot did not compute"; see `commands::adopt`'s
    /// deployment conflict check.
    pub unrendered: Vec<PathBuf>,
    /// Files whose deployed bytes diverged from the cached baseline and
    /// were therefore preserved instead of being overwritten. Empty
    /// outside of `dodot up` runs that pass `force = false` and have a
    /// baseline available. Surfaced to the user as warnings — see
    /// `docs/proposals/preprocessing-pipeline.lex` §6.4.
    pub skipped: Vec<SkippedRender>,
}

/// One file the pipeline refused to overwrite because its deployed
/// bytes diverged from the cached render.
///
/// `dodot up` records these so the caller can warn the user that their
/// edits were preserved. Resolution paths are `dodot transform check`
/// (auto-merge via the clean filter) or `dodot up --force` (overwrite).
#[derive(Debug, Clone)]
pub struct SkippedRender {
    /// Pack name (matches `Pack::name`, the on-disk directory name).
    pub pack: String,
    /// Virtual relative path inside the pack (post-strip), e.g.
    /// `config.toml` for a source `config.toml.tmpl`.
    pub virtual_relative: PathBuf,
    /// Absolute path of the deployed file we preserved.
    pub deployed_path: PathBuf,
    /// Which divergence state we observed. Always `OutputChanged` or
    /// `BothChanged` — the other states never trigger a skip.
    pub state: DivergenceState,
}

impl PreprocessResult {
    /// Create a passthrough result where all entries are regular (no preprocessing).
    pub fn passthrough(entries: Vec<PackEntry>) -> Self {
        Self {
            regular_entries: entries,
            virtual_entries: Vec::new(),
            source_map: HashMap::new(),
            rendered_bytes: HashMap::new(),
            unrendered: Vec::new(),
            skipped: Vec::new(),
        }
    }

    /// Return all entries (regular + virtual) merged into one list, sorted by relative path.
    pub fn merged_entries(&self) -> Vec<PackEntry> {
        let mut all = Vec::with_capacity(self.regular_entries.len() + self.virtual_entries.len());
        all.extend(self.regular_entries.iter().cloned());
        all.extend(self.virtual_entries.iter().cloned());
        all.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        all
    }
}

/// The handler name used for preprocessor-expanded files in the datastore.
const PREPROCESSED_HANDLER: &str = "preprocessed";

/// Result of checking whether the deployed file diverges from the
/// cached baseline. Used by [`preprocess_pack`] to decide whether to
/// overwrite or preserve the user's edits.
enum DivergenceCheck {
    /// No baseline, no deployed file, or content matches — proceed
    /// with the normal write.
    Proceed,
    /// Deployed bytes diverge from the baseline. Skip the write to
    /// preserve user edits; surface a warning to the caller.
    Skip {
        state: DivergenceState,
        deployed_path: PathBuf,
    },
}

/// Compare the prospective deployed file against the cached baseline.
///
/// Returns [`DivergenceCheck::Skip`] when the deployed bytes have
/// changed since the last successful render — that is the case where
/// re-rendering would silently destroy a user edit (see
/// `docs/proposals/preprocessing-pipeline.lex` §6.4).
///
/// "Define stale-vs-new from file content, not the runtime
/// environment": this check operates purely on bytes (source + deployed
/// hash comparisons against the baseline). Env-var rotations are
/// intentionally invisible here — users who change a referenced env var
/// pick up the new value via `dodot up --force`.
fn check_divergence(
    fs: &dyn Fs,
    paths: &dyn Pather,
    pack_name: &str,
    virtual_relative: &Path,
    source_path: &Path,
) -> Result<DivergenceCheck> {
    let cache_filename = cache_filename_for(virtual_relative);
    let baseline =
        match Baseline::load(fs, paths, pack_name, PREPROCESSED_HANDLER, &cache_filename)? {
            Some(b) => b,
            // First-time deploy: no baseline to compare against. Writing
            // is correct here — there's nothing to overwrite.
            None => return Ok(DivergenceCheck::Proceed),
        };

    let deployed_path = paths
        .handler_data_dir(pack_name, PREPROCESSED_HANDLER)
        .join(virtual_relative);
    if !fs.exists(&deployed_path) {
        // Baseline says we deployed once, but the user (or some other
        // tool) removed the deployed file. Treat as a fresh deploy —
        // there's nothing to preserve.
        return Ok(DivergenceCheck::Proceed);
    }

    let deployed_bytes = fs.read_file(&deployed_path)?;
    if hex_sha256(&deployed_bytes) == baseline.rendered_hash {
        return Ok(DivergenceCheck::Proceed);
    }

    // Deployed file diverges. Distinguish OutputChanged from BothChanged
    // for a sharper warning. A read failure on the source is treated as
    // "source unchanged" — the safer assumption when we can't tell.
    let source_changed = match fs.read_file(source_path) {
        Ok(bytes) => hex_sha256(&bytes) != baseline.source_hash,
        Err(_) => false,
    };
    let state = if source_changed {
        DivergenceState::BothChanged
    } else {
        DivergenceState::OutputChanged
    };

    Ok(DivergenceCheck::Skip {
        state,
        deployed_path,
    })
}

/// Run the preprocessing pipeline for a pack's file entries.
///
/// 1. Partition entries into preprocessor files vs regular files.
/// 2. **In `PreprocessMode::Active`** (real `dodot up` runs): for each
///    preprocessor file, expand, write results to datastore (unless the
///    deployed file has diverged from the cached baseline — see step 5),
///    write the baseline cache record.
/// 3. Create virtual `PackEntry`s pointing to the datastore files.
/// 4. Check for collisions between virtual and regular entries.
/// 5. **Divergence guard** (Active only): unless `force` is `true`,
///    compare the prospective deployed file against the cached baseline
///    before overwriting. When the deployed bytes have changed (the
///    user edited the deployed file directly), skip the write and
///    record a [`SkippedRender`] so the caller can warn the user. See
///    `docs/proposals/preprocessing-pipeline.lex` §6.4.
/// 6. **In `PreprocessMode::Passive`** (`dodot status`, `up --dry-run`):
///    skip every disk-mutating step. Sources are never read for marker
///    scans; preprocessors are never invoked (no provider calls); the
///    datastore is not touched. Virtual entries are still produced so
///    the rest of the planner can compute intents — their bytes come
///    from `baseline.rendered_content` when a baseline exists, and a
///    baseline rendered from source bytes or a context that have since
///    changed is additionally recorded in
///    [`PreprocessResult::unrendered`], because those bytes are the
///    previous render rather than the next one.
///    First-time pack templates with no baseline still surface a
///    placeholder virtual entry (so `dodot status` can render them as
///    "pending" under the stripped name) but with empty
///    `rendered_bytes`. Handlers that need rendered content for
///    sentinel hashing (`install`, `homebrew`) skip intent generation
///    for those placeholders rather than erroring out — the next real
///    `dodot up` plans them normally. See [`PreprocessMode`] and
///    `docs/proposals/secrets.lex` §7.4.
/// 7. Return the result for merging into the handler pipeline.
///
/// Set `force = true` to bypass the divergence guard. Surfaces as
/// `dodot up --force` in the CLI; needed when the user knows they want
/// to overwrite a divergent deployed file (e.g. after rotating an env
/// var that a template references). Ignored in `Passive` mode (no
/// writes happen there at all).
#[allow(clippy::too_many_arguments)] // pipeline core: every parameter is load-bearing
pub fn preprocess_pack(
    entries: Vec<PackEntry>,
    registry: &PreprocessorRegistry,
    pack: &Pack,
    fs: &dyn Fs,
    datastore: &dyn DataStore,
    paths: &dyn Pather,
    mode: PreprocessMode,
    force: bool,
) -> Result<PreprocessResult> {
    let mut regular_entries = Vec::new();
    let mut preprocessor_entries = Vec::new();

    // Phase 1: Partition
    for entry in entries {
        // Gate-failed entries (basename or directory-segment) must never
        // reach the template engine. Route them straight to regular_entries
        // so match_entries can emit the gate-handler match for status, but
        // the preprocessor never sees them. Without this guard, a template
        // like `aliases._linux.sh.tmpl` on a darwin host would be sent to
        // MiniJinja, which triggers strict-undefined render failures,
        // secret-provider calls, and baseline-cache writes — all of which
        // the user opted out of by using a gate.
        if entry.gate_failure.is_some() {
            regular_entries.push(entry);
            continue;
        }

        let filename = entry
            .relative_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if !entry.is_dir && registry.is_preprocessor_file(&filename) {
            preprocessor_entries.push(entry);
        } else {
            regular_entries.push(entry);
        }
    }

    debug!(
        pack = %pack.name,
        preprocessor = preprocessor_entries.len(),
        regular = regular_entries.len(),
        "partitioned entries"
    );

    if preprocessor_entries.is_empty() {
        return Ok(PreprocessResult {
            regular_entries,
            virtual_entries: Vec::new(),
            source_map: HashMap::new(),
            rendered_bytes: HashMap::new(),
            unrendered: Vec::new(),
            skipped: Vec::new(),
        });
    }

    // Passive mode: read everything from the baseline cache. Skip
    // template evaluation entirely (no provider calls), skip
    // datastore writes, skip baseline writes. See `PreprocessMode`.
    if mode == PreprocessMode::Passive {
        return preprocess_pack_passive(
            preprocessor_entries,
            regular_entries,
            registry,
            pack,
            fs,
            paths,
        );
    }

    // Phase 2 & 3: Expand and create virtual entries
    let mut virtual_entries = Vec::new();
    let mut source_map = HashMap::new();
    let mut rendered_bytes: HashMap<PathBuf, Arc<[u8]>> = HashMap::new();
    let mut skipped: Vec<SkippedRender> = Vec::new();

    // Tracks claimed paths for collision detection. Seeded with regular
    // entries; virtual entries are added as they're created so two
    // preprocessors can't both produce the same virtual path (e.g.
    // `config.toml.identity` and `config.toml.tmpl` both expanding to
    // `config.toml`).
    let mut claimed_paths: std::collections::HashSet<PathBuf> = regular_entries
        .iter()
        .map(|e| e.relative_path.clone())
        .collect();

    for entry in &preprocessor_entries {
        let filename = entry
            .relative_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let preprocessor = registry
            .find_for_file(&filename)
            .expect("already checked in partition");

        info!(
            pack = %pack.name,
            preprocessor = preprocessor.name(),
            file = %filename,
            "expanding"
        );

        // Safety gate (see `conflict::ensure_no_unresolved_markers`
        // for the lossy-decode contract): refuse to expand a source
        // carrying unresolved dodot-conflict markers, which would
        // otherwise render verbatim and deploy as broken config.
        // Gated on `supports_reverse_merge` so non-tracking
        // preprocessors (unarchive, identity) don't pay the read cost
        // — their sources can't naturally carry the marker token.
        // See preprocessing-pipeline.lex §6.3.
        if preprocessor.supports_reverse_merge() {
            let source_bytes = fs.read_file(&entry.absolute_path)?;
            let source_str = String::from_utf8_lossy(&source_bytes);
            crate::preprocessing::conflict::ensure_no_unresolved_markers(
                &source_str,
                &entry.absolute_path,
            )?;
        }

        let expanded_files = preprocessor.expand(&entry.absolute_path, fs)?;

        for expanded in expanded_files {
            validate_safe_relative_path(
                &expanded.relative_path,
                preprocessor.name(),
                &entry.absolute_path,
            )?;

            // A source in a subdirectory (e.g. "subdir/config.toml.identity")
            // keeps its parent in the virtual entry ("subdir/config.toml").
            let virtual_relative = if let Some(parent) = entry.relative_path.parent() {
                if parent == Path::new("") {
                    expanded.relative_path.clone()
                } else {
                    parent.join(&expanded.relative_path)
                }
            } else {
                expanded.relative_path.clone()
            };

            // Defense-in-depth: validate the joined path too (parent
            // could only come from the pack scanner, but re-check).
            validate_safe_relative_path(
                &virtual_relative,
                preprocessor.name(),
                &entry.absolute_path,
            )?;

            // Normalise `./foo` and `foo` to the same canonical form, so
            // that collision detection and downstream comparisons don't
            // silently diverge from the datastore's own normalisation.
            let virtual_relative = normalize_relative(&virtual_relative);

            // Phase 4: Collision check (against both regular entries and
            // previously-expanded virtual entries)
            if claimed_paths.contains(&virtual_relative) {
                return Err(DodotError::PreprocessorCollision {
                    pack: pack.name.clone(),
                    source_file: filename.clone(),
                    expanded_name: virtual_relative.to_string_lossy().into_owned(),
                });
            }

            // The divergence guard (§6.4) runs *after*
            // `preprocessor.expand` above: moving it ahead of expansion
            // would require knowing every output path before producing
            // any of them, which the preprocessor contract doesn't
            // expose. The cost of the spurious render is the cycles
            // burned plus any one-shot side effects in expand (e.g.
            // secret-provider prompts for templates that resolve
            // `{{ secrets.X }}`). For divergent files the prompt fires
            // even though the rendered bytes are immediately discarded;
            // users who want to avoid that should resolve the
            // divergence (`dodot transform check`) before the next
            // `dodot up`.
            let mut skip_path: Option<PathBuf> = None;
            // Divergence-guard gate: fires for any preprocessor
            // that produces a single file we can hash against the
            // baseline. Templates use `tracked_render` (so they
            // also get reverse-merge); whole-file secret
            // preprocessors (`age` / `gpg`) signal participation
            // via `deploy_mode = Some(0o600)`. `secrets.lex` §4.4
            // is explicit that whole-file secrets must NOT have
            // their deployed plaintext silently overwritten on the
            // next `dodot up` — even though there's no auto-merge
            // path, the §6.4 preservation contract still applies.
            let participates_in_divergence_guard =
                expanded.tracked_render.is_some() || expanded.deploy_mode.is_some();
            if !force && !expanded.is_dir && participates_in_divergence_guard {
                match check_divergence(
                    fs,
                    paths,
                    &pack.name,
                    &virtual_relative,
                    &entry.absolute_path,
                )? {
                    DivergenceCheck::Proceed => {}
                    DivergenceCheck::Skip {
                        state,
                        deployed_path,
                    } => {
                        info!(
                            pack = %pack.name,
                            file = %virtual_relative.display(),
                            ?state,
                            "preserving divergent deployed file (skipping write)"
                        );
                        skipped.push(SkippedRender {
                            pack: pack.name.clone(),
                            virtual_relative: virtual_relative.clone(),
                            deployed_path: deployed_path.clone(),
                            state,
                        });
                        skip_path = Some(deployed_path);
                    }
                }
            }
            let was_skipped = skip_path.is_some();

            let datastore_path = if let Some(p) = skip_path {
                p
            } else if expanded.is_dir {
                datastore.write_rendered_dir(
                    &pack.name,
                    PREPROCESSED_HANDLER,
                    &virtual_relative.to_string_lossy(),
                )?
            } else if let Some(mode) = expanded.deploy_mode {
                // Whole-file secret preprocessors (age / gpg) emit
                // `deploy_mode = Some(0o600)` per `secrets.lex`
                // §4.3. Use the atomic create-with-mode datastore
                // path so the plaintext bytes never sit on disk
                // under a permissive mode — closes the race window
                // between `write_file` (lands at umask default,
                // typically 0644) and `set_permissions` that the
                // first cut had.
                datastore.write_rendered_file_with_mode(
                    &pack.name,
                    PREPROCESSED_HANDLER,
                    &virtual_relative.to_string_lossy(),
                    &expanded.content,
                    mode,
                )?
            } else {
                datastore.write_rendered_file(
                    &pack.name,
                    PREPROCESSED_HANDLER,
                    &virtual_relative.to_string_lossy(),
                    &expanded.content,
                )?
            };

            debug!(
                pack = %pack.name,
                virtual_path = %virtual_relative.display(),
                datastore_path = %datastore_path.display(),
                is_dir = expanded.is_dir,
                skipped = was_skipped,
                "wrote expanded entry"
            );

            // Persist a baseline record so future `dodot transform
            // check` / clean-filter calls can detect drift without
            // re-rendering. The gate mirrors the divergence guard, so
            // the guard has data to compare against next run. Only
            // write when:
            //   - the entry is a file (directory entries from archive
            //     preprocessors carry no rendered content),
            //   - the preprocessor participates: templates supply
            //     `tracked_render` (which both unlocks reverse-merge
            //     and seeds the baseline), whole-file secrets supply
            //     `deploy_mode` (no marker stream, but rendered_hash
            //     is still meaningful for divergence detection per
            //     `secrets.lex` §4.4). Preprocessors with neither
            //     (unarchive) skip the baseline, AND
            //   - the divergence guard didn't skip the write (otherwise
            //     we'd update the baseline to match a render that never
            //     hit disk, breaking future divergence detection).
            //
            // Reached only in `PreprocessMode::Active` — Passive takes
            // the early return at the top of the function.
            let should_write_baseline = !expanded.is_dir
                && !was_skipped
                && (expanded.tracked_render.is_some() || expanded.deploy_mode.is_some());
            if should_write_baseline {
                let cache_filename = cache_filename_for(&virtual_relative);
                let source_bytes = fs.read_file(&entry.absolute_path)?;
                let baseline = Baseline::build(
                    &entry.absolute_path,
                    &expanded.content,
                    &source_bytes,
                    expanded.tracked_render.as_deref(),
                    expanded.context_hash.as_ref(),
                );
                if let Err(err) =
                    baseline.write(fs, paths, &pack.name, PREPROCESSED_HANDLER, &cache_filename)
                {
                    // Baseline write failures are reported but not
                    // fatal: the deployment itself succeeded, and a
                    // missing baseline only degrades the reverse-merge
                    // experience (we'll re-baseline next `up`).
                    debug!(
                        pack = %pack.name,
                        file = %cache_filename,
                        error = %err,
                        "baseline write failed (non-fatal)"
                    );
                } else {
                    debug!(
                        pack = %pack.name,
                        file = %cache_filename,
                        "baseline written"
                    );
                }

                // Secrets sidecar (secrets.lex §3.3). Always called so
                // the on-disk state matches the latest render; see
                // `SecretsSidecar::write` for the no-secrets case.
                let sidecar = crate::preprocessing::baseline::SecretsSidecar::new(
                    expanded.secret_line_ranges.clone(),
                );
                if let Err(err) =
                    sidecar.write(fs, paths, &pack.name, PREPROCESSED_HANDLER, &cache_filename)
                {
                    // Same non-fatal disposition as baseline writes:
                    // a missing sidecar means the next reverse-merge
                    // sees an empty mask and surfaces the secret
                    // line as a regular (mask-able) divergence,
                    // which the user can recover from by re-running
                    // `dodot up`.
                    debug!(
                        pack = %pack.name,
                        file = %cache_filename,
                        error = %err,
                        "secrets sidecar write failed (non-fatal)"
                    );
                }
            }

            claimed_paths.insert(virtual_relative.clone());
            source_map.insert(datastore_path.clone(), entry.absolute_path.clone());
            // Stash the rendered bytes for downstream handlers
            // (install/homebrew sentinel hashing) that would
            // otherwise read them back off disk. Skipped renders
            // (divergence guard fired) carry the *preserved deployed*
            // bytes instead — that matches the deployed file the user
            // is keeping, which is what the next sentinel should
            // commit to. Directories carry no bytes.
            if !expanded.is_dir {
                let bytes: Arc<[u8]> = if was_skipped {
                    // Read the preserved deployed file. If the read
                    // fails (race / permissions), fall back to the
                    // freshly-rendered bytes so the handler still
                    // gets a value — this only affects the sentinel,
                    // and the divergence warning has already surfaced.
                    fs.read_file(&datastore_path)
                        .map(Arc::from)
                        .unwrap_or_else(|_| Arc::from(expanded.content.clone()))
                } else {
                    Arc::from(expanded.content.clone())
                };
                rendered_bytes.insert(datastore_path.clone(), bytes);
            }

            virtual_entries.push(PackEntry {
                relative_path: virtual_relative,
                absolute_path: datastore_path,
                is_dir: expanded.is_dir,
                gate_failure: None,
            });
        }
    }

    info!(
        pack = %pack.name,
        virtual_count = virtual_entries.len(),
        "preprocessing complete"
    );

    Ok(PreprocessResult {
        regular_entries,
        virtual_entries,
        source_map,
        rendered_bytes,
        // Active rendered every entry it surfaced.
        unrendered: Vec::new(),
        skipped,
    })
}

/// `Passive` half of [`preprocess_pack`].
///
/// Walks the same set of preprocessor entries the Active path would
/// have, but never invokes a preprocessor. For each entry, computes
/// the would-be virtual relative path via `Preprocessor::stripped_name`.
/// Two outcomes:
///
/// - **Baseline exists** (the file was rendered on a previous `up`):
///   builds a virtual entry pointing at the would-be datastore
///   location with `rendered_bytes` sourced from
///   `baseline.rendered_content`. Runs the read-only divergence
///   check so callers (status's `Health::Preserved` row) still see
///   skipped-render rows for divergent deployed files. The baseline's
///   own inputs are checked too ([`superseded_reason`]): a template
///   edited since the last `up`, or one whose `vars` changed, has a
///   baseline holding the *previous* render, so the entry is also
///   recorded in [`PreprocessResult::unrendered`] — it keeps its
///   bytes (they are what is deployed) while telling callers that the
///   claims it yields are last render's, not this source's.
/// - **No baseline** (first-time pack template, never `up`'d):
///   surfaces a placeholder virtual entry under the stripped name,
///   with no `rendered_bytes` and the entry recorded in
///   [`PreprocessResult::unrendered`]. Status renders this as
///   "pending" under the logical name (`config.toml` rather than the
///   source `config.toml.tmpl`); handlers that need rendered content
///   for sentinel hashing (install, homebrew, nix) skip intent
///   generation for these placeholders rather than crashing, and so
///   does `externals`, whose targets are declared inside the file. The
///   next real `dodot up` populates the baseline and plans intents
///   normally.
///
///   A placeholder therefore makes the resulting plan *incomplete*,
///   not merely empty, for the handlers that skip it. Callers that
///   read a plan to prove something about a pack before mutating —
///   adopt's cross-pack conflict analysis — must consult `unrendered`
///   and refuse rather than read "no intent" as "no claim".
///
/// Source files are read only to be hashed against the baseline (no
/// marker scan, no expansion); the datastore is not written; the
/// baseline cache is not written.
///
/// This contract is what `secrets.lex` §7.4 demands: `dodot status`
/// and `dodot up --dry-run` MUST NOT trigger template evaluation,
/// MUST NOT surface provider auth prompts, and MUST NOT mutate disk
/// state.
///
/// Limitation: this assumes a 1:1 source→virtual relationship via
/// `stripped_name`. That holds for templates (the only shipped
/// generative-with-tracking preprocessor) and identity-style
/// preprocessors. Multi-output preprocessors like unarchive cannot
/// faithfully be passively previewed; if one is added later, this
/// function should fall back to skipping such entries (which it does
/// today, since they have no baseline).
fn preprocess_pack_passive(
    preprocessor_entries: Vec<PackEntry>,
    regular_entries: Vec<PackEntry>,
    registry: &PreprocessorRegistry,
    pack: &Pack,
    fs: &dyn Fs,
    paths: &dyn Pather,
) -> Result<PreprocessResult> {
    let mut virtual_entries = Vec::new();
    let mut source_map = HashMap::new();
    let mut rendered_bytes: HashMap<PathBuf, Arc<[u8]>> = HashMap::new();
    let mut unrendered: Vec<PathBuf> = Vec::new();
    let mut skipped: Vec<SkippedRender> = Vec::new();

    for entry in preprocessor_entries {
        let filename = entry
            .relative_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let preprocessor = registry
            .find_for_file(&filename)
            .expect("already checked in partition");

        // Logical (stripped) virtual filename — e.g. `config.toml`
        // for `config.toml.tmpl`. We don't run `expand()` (that would
        // be the §7.4 violation), so we derive the would-be virtual
        // path from `stripped_name` plus the source's parent
        // directory.
        let stripped = preprocessor.stripped_name(&filename);
        let virtual_relative = match entry.relative_path.parent() {
            Some(parent) if parent != Path::new("") => parent.join(&stripped),
            _ => PathBuf::from(&stripped),
        };
        let virtual_relative = normalize_relative(&virtual_relative);

        let datastore_path = paths
            .handler_data_dir(&pack.name, PREPROCESSED_HANDLER)
            .join(&virtual_relative);

        // A missing baseline means a first-time template that has
        // never been deployed. It surfaces as a placeholder virtual
        // entry with no rendered bytes; falling through to template
        // evaluation here would be the §7.4 violation this path
        // exists to avoid.
        let cache_filename = cache_filename_for(&virtual_relative);
        let baseline =
            match Baseline::load(fs, paths, &pack.name, PREPROCESSED_HANDLER, &cache_filename)? {
                Some(b) => Some(b),
                None => {
                    debug!(
                        pack = %pack.name,
                        file = %virtual_relative.display(),
                        "passive: no baseline yet — surfacing placeholder (run `dodot up` first)"
                    );
                    None
                }
            };

        // Divergence detection (read-only): even though Passive
        // never writes, status / dry-run callers want to know which
        // deployed files have drifted from their baseline so they
        // can surface the same `Health::Preserved` row that the
        // active path does. The byte comparison is local and free
        // of side effects — no provider calls, no template eval —
        // so it stays inside the §7.4 envelope.
        if baseline.is_some() {
            if let Ok(DivergenceCheck::Skip {
                state,
                deployed_path,
            }) = check_divergence(
                fs,
                paths,
                &pack.name,
                &virtual_relative,
                &entry.absolute_path,
            ) {
                skipped.push(SkippedRender {
                    pack: pack.name.clone(),
                    virtual_relative: virtual_relative.clone(),
                    deployed_path,
                    state,
                });
            }
        }

        // Carry the baseline's rendered content forward as the
        // in-memory bytes for downstream sentinel hashing. Without a
        // baseline, handlers fall back to a disk read that correctly
        // fails for the missing datastore file and shows up as
        // "pending" in status.
        match baseline {
            Some(b) => {
                // The bytes describe what is deployed right now, which
                // is what status reports — but they describe the last
                // render's inputs, and those can have moved on. When
                // they have, the entry joins the never-rendered ones:
                // still offering its bytes to status, and still telling
                // a caller that reads the plan as evidence that this
                // file's current claims are not in it.
                if let Some(reason) = superseded_reason(fs, preprocessor, &entry.absolute_path, &b)
                {
                    debug!(
                        pack = %pack.name,
                        file = %virtual_relative.display(),
                        reason,
                        "passive: cached render no longer describes this source \
                         (run `dodot up` to re-render)"
                    );
                    unrendered.push(datastore_path.clone());
                }
                let bytes: Arc<[u8]> = Arc::from(b.rendered_content.into_bytes());
                rendered_bytes.insert(datastore_path.clone(), bytes);
            }
            // No bytes to offer. Record the entry so callers can tell
            // "this file claims nothing" apart from "dodot has not
            // computed what this file claims" — see
            // `PreprocessResult::unrendered`.
            None => unrendered.push(datastore_path.clone()),
        }
        source_map.insert(datastore_path.clone(), entry.absolute_path.clone());
        virtual_entries.push(PackEntry {
            relative_path: virtual_relative,
            absolute_path: datastore_path,
            is_dir: false,
            gate_failure: None,
        });
    }

    info!(
        pack = %pack.name,
        virtual_count = virtual_entries.len(),
        skipped_count = skipped.len(),
        "passive preprocessing complete"
    );

    Ok(PreprocessResult {
        regular_entries,
        virtual_entries,
        source_map,
        rendered_bytes,
        unrendered,
        skipped,
    })
}

/// Why a cached render no longer describes what rendering the source
/// would produce now — or `None` when it still does.
///
/// Two inputs decide a render and can both move without touching the
/// datastore: the source bytes, hashed into
/// [`Baseline::source_hash`], and everything else the preprocessor
/// reads, hashed into [`Baseline::context_hash`] (for templates, the
/// `dodot.*` namespace and the configured `vars`). Either one moving
/// means the next `dodot up` produces different output than the
/// baseline holds — and for a file that declares its deployment
/// targets in its own contents, different output can mean different
/// targets.
///
/// An unreadable source counts as superseded. The divergence walker
/// makes the opposite call for the same comparison, because a report
/// row is cheap to be wrong about and the user re-runs; here the
/// answer decides whether adopt may mutate the dotfiles tree, and a
/// source dodot cannot read is a render it cannot vouch for.
///
/// A baseline written before `context_hash` existed carries an empty
/// string, and a preprocessor with no context of its own reports
/// `None`; neither case can be compared, so both are left to the
/// source-bytes check alone.
///
/// Reading the source here is a hash, not an expansion: no template
/// evaluation, no provider calls, nothing written — inside the
/// `secrets.lex` §7.4 envelope.
fn superseded_reason(
    fs: &dyn Fs,
    preprocessor: &dyn crate::preprocessing::Preprocessor,
    source_path: &Path,
    baseline: &Baseline,
) -> Option<&'static str> {
    match fs.read_file(source_path) {
        Ok(bytes) if hex_sha256(&bytes) != baseline.source_hash => {
            return Some("source bytes changed since the cached render")
        }
        Err(_) => return Some("source file could not be read"),
        Ok(_) => {}
    }

    let current = preprocessor.context_hash().as_ref().map(hex_encode_32);
    match current {
        Some(current) if !baseline.context_hash.is_empty() && current != baseline.context_hash => {
            Some("rendering context changed since the cached render")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
