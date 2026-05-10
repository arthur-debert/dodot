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
use crate::preprocessing::baseline::{cache_filename_for, hex_sha256, Baseline};
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
/// This enum is the single boolean the pipeline gates on. Active is
/// the existing behavior; Passive is the §7.4-compliant read-only
/// path. See issue #121.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreprocessMode {
    /// Run preprocessors, write rendered outputs to the datastore,
    /// write baselines to the cache. The original `dodot up` path.
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
    /// Populated for every virtual entry the pipeline produces, in
    /// both Active and Passive modes (Passive sources the bytes from
    /// `baseline.rendered_content`). Handlers that need the rendered
    /// content for sentinel hashing (`install`, `homebrew`) consult
    /// this map first and fall back to disk read for non-template
    /// files. Without this, Passive callers — where the rendered
    /// file isn't on disk — couldn't produce correct sentinels for
    /// templated install scripts or Brewfiles. See issue #121.
    pub rendered_bytes: HashMap<PathBuf, Arc<[u8]>>,
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
///    from `baseline.rendered_content` when a baseline exists.
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

        // Safety gate: refuse to expand a source carrying unresolved
        // dodot-conflict markers. Otherwise the markers would render
        // verbatim through the template engine and deploy as broken
        // config. Gated on `supports_reverse_merge` so non-tracking
        // preprocessors (unarchive, identity) don't pay the read cost
        // — their sources can't naturally carry the marker token.
        //
        // Lossy UTF-8 conversion: we read raw bytes and decode lossily
        // so a non-UTF-8 source for a reverse-merge-capable
        // preprocessor still gets a clean scan rather than failing
        // with a generic UTF-8 decode error. The marker token is
        // ASCII, so the lossy decode preserves it. Templates today
        // are always UTF-8 in practice; this is defence-in-depth for
        // future preprocessors.
        // See preprocessing-pipeline.lex §6.3.
        if preprocessor.supports_reverse_merge() {
            let source_bytes = fs.read_file(&entry.absolute_path)?;
            let source_str = String::from_utf8_lossy(&source_bytes);
            crate::preprocessing::conflict::ensure_no_unresolved_markers(
                &source_str,
                &entry.absolute_path,
            )?;
        }

        // Expand the source file
        let expanded_files = preprocessor.expand(&entry.absolute_path, fs)?;

        for expanded in expanded_files {
            // Reject unsafe paths from the preprocessor (tar-slip,
            // absolute paths, parent-dir escapes) before any disk write.
            validate_safe_relative_path(
                &expanded.relative_path,
                preprocessor.name(),
                &entry.absolute_path,
            )?;

            // Compute the virtual relative path.
            // If the source was in a subdirectory (e.g., "subdir/config.toml.identity"),
            // the virtual entry should preserve the parent (e.g., "subdir/config.toml").
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

            // Write expanded content to datastore, preserving directory
            // structure. Directories get mkdir'd; files get their content
            // written. `write_rendered_file` creates any needed parent
            // directories.
            //
            // Divergence guard (§6.4): for tracked-render preprocessors,
            // check whether the deployed file has diverged from the
            // cached baseline before overwriting. If it has, skip the
            // *write* and record a SkippedRender so the caller can warn
            // the user. `force = true` bypasses the guard. See
            // `check_divergence` for the byte-level rule.
            //
            // The render itself (`preprocessor.expand` above) has
            // already run by this point — moving the divergence check
            // ahead of expansion would require knowing every output
            // path before producing any of them, which the preprocessor
            // contract doesn't expose. The cost of the spurious render
            // is the cycles burned plus any one-shot side effects in
            // expand (e.g. secret-provider prompts for templates that
            // resolve `{{ secrets.X }}`). For divergent files this
            // means the prompt fires even though the rendered bytes
            // are immediately discarded; users who want to avoid that
            // should resolve the divergence (`dodot transform check`)
            // before the next `dodot up`. Tracked here for §6.4
            // follow-up; not blocking the divergence-preservation
            // contract this guard exists to keep.
            //
            // The guard fires regardless of `write_baselines` — it's a
            // read-only check against the existing cache, and read-only
            // callers (`dodot status`) need it just as much as `dodot
            // up` does. Without this, status would re-render and
            // overwrite the user's edited deployed file silently.
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
            // re-rendering. Only write when:
            //   - the entry is a file (directory entries from archive
            //     preprocessors carry no rendered content),
            //   - the preprocessor produced a tracked render (i.e. it's
            //     a generative-with-tracking preprocessor, currently
            //     just templates). Plain Generative preprocessors that
            //     don't support reverse-merge (unarchive) skip the
            //     baseline because the cache is only meaningful when
            //     paired with burgertocow tracking, AND
            //   - the divergence guard didn't skip the write (otherwise
            //     we'd update the baseline to match a render that never
            //     hit disk, breaking future divergence detection).
            //
            // Mode-gating happens at the function boundary: this whole
            // branch only runs in `PreprocessMode::Active`. Passive
            // commands take the early-return at the top of the
            // function and never reach this code.
            // Baseline-write gate: write whenever the divergence
            // guard would fire next time, so the guard has data to
            // compare against. Templates supply `tracked_render`
            // (which both unlocks reverse-merge and seeds the
            // baseline); whole-file secrets supply `deploy_mode`
            // (no marker stream — `tracked_render = None` — but
            // rendered_hash is still meaningful for divergence
            // detection per `secrets.lex` §4.4).
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

                // Secrets sidecar (secrets.lex §3.3). Always called;
                // the writer no-ops when the render had no
                // `secret(...)` calls AND removes a stale sidecar
                // from a prior render that DID, so the on-disk
                // state always matches the latest render.
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
///   skipped-render rows for divergent deployed files.
/// - **No baseline** (first-time pack template, never `up`'d):
///   surfaces a placeholder virtual entry under the stripped name,
///   with empty `rendered_bytes`. Status renders this as "pending"
///   under the logical name (`config.toml` rather than the source
///   `config.toml.tmpl`); handlers that need rendered content for
///   sentinel hashing (install, homebrew) skip intent generation
///   for these placeholders rather than crashing. The next real
///   `dodot up` populates the baseline and plans intents normally.
///
/// Source files are not read (no marker scan); the datastore is
/// not written; the baseline cache is not written.
///
/// This contract is what `secrets.lex` §7.4 demands: `dodot status`
/// and `dodot up --dry-run` MUST NOT trigger template evaluation,
/// MUST NOT surface provider auth prompts, and MUST NOT mutate disk
/// state. See issue #121.
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

        // Try to load the cached baseline. If absent, this is a
        // first-time template that has never been deployed: surface
        // a placeholder virtual entry (no rendered_bytes) so callers
        // like `dodot status` can render it as "pending" under the
        // stripped name. Critically, we do NOT fall through to
        // template evaluation — that's the §7.4 violation we're
        // here to fix. Handlers that need rendered bytes for
        // sentinel hashing (`install`, `homebrew`) will fall back
        // to disk-read on the missing datastore path and report
        // pending; symlink-targeted templates render cleanly as
        // pending without needing the bytes at all.
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
        // so it stays inside the §7.4 envelope. Skipped only when a
        // baseline exists (no baseline → no comparison reference).
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
        // in-memory bytes for downstream sentinel hashing when a
        // baseline exists. Without a baseline (first-time pack), no
        // bytes are available — handlers that need them will see
        // `m.rendered_bytes == None` and fall back to disk read,
        // which correctly fails for the missing datastore file and
        // shows up as "pending" in status.
        if let Some(b) = baseline {
            let bytes: Arc<[u8]> = Arc::from(b.rendered_content.into_bytes());
            rendered_bytes.insert(datastore_path.clone(), bytes);
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
        skipped,
    })
}

#[cfg(test)]
mod tests;
