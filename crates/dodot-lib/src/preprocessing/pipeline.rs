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
///    First-time pack templates with no baseline are silently skipped
///    (status shows nothing for them; the user runs `dodot up` first).
///    See [`PreprocessMode`] and `docs/proposals/secrets.lex` §7.4.
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
            if !force && !expanded.is_dir && expanded.tracked_render.is_some() {
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
            if let (false, Some(tracked), false) = (
                expanded.is_dir,
                expanded.tracked_render.as_deref(),
                was_skipped,
            ) {
                let cache_filename = cache_filename_for(&virtual_relative);
                let source_bytes = fs.read_file(&entry.absolute_path)?;
                let baseline = Baseline::build(
                    &entry.absolute_path,
                    &expanded.content,
                    &source_bytes,
                    Some(tracked),
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
/// the would-be virtual relative path via `Preprocessor::stripped_name`
/// and looks up the cached baseline. When a baseline exists, builds a
/// virtual entry pointing at the would-be datastore location with
/// `rendered_bytes` sourced from `baseline.rendered_content`. When no
/// baseline exists (first-time pack template), the entry is silently
/// skipped — passive callers display nothing for un-baselined templates,
/// the user runs `dodot up` first. Source files are not read (no marker
/// scan); the datastore is not written; the baseline cache is not
/// written.
///
/// This contract is what `secrets.lex` §7.4 demands: `dodot status` and
/// `dodot up --dry-run` MUST NOT trigger template evaluation, MUST NOT
/// surface provider auth prompts, and MUST NOT mutate disk state. See
/// issue #121.
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
mod tests {
    use super::*;
    use crate::datastore::FilesystemDataStore;
    use crate::handlers::HandlerConfig;
    use crate::preprocessing::identity::IdentityPreprocessor;
    use crate::testing::TempEnvironment;
    use std::sync::Arc;

    fn make_pack(name: &str, path: PathBuf) -> Pack {
        Pack::new(name.into(), path, HandlerConfig::default())
    }

    fn make_registry() -> PreprocessorRegistry {
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(IdentityPreprocessor::new()));
        registry
    }

    fn make_datastore(env: &TempEnvironment) -> FilesystemDataStore {
        let runner = Arc::new(crate::datastore::ShellCommandRunner::new(false));
        FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), runner)
    }

    #[test]
    fn passthrough_when_no_preprocessor_files() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("gvimrc", "set guifont=Mono")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("vim", env.dotfiles_root.join("vim"));

        let entries = vec![
            PackEntry {
                relative_path: "vimrc".into(),
                absolute_path: env.dotfiles_root.join("vim/vimrc"),
                is_dir: false,
            },
            PackEntry {
                relative_path: "gvimrc".into(),
                absolute_path: env.dotfiles_root.join("vim/gvimrc"),
                is_dir: false,
            },
        ];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert_eq!(result.regular_entries.len(), 2);
        assert!(result.virtual_entries.is_empty());
        assert!(result.source_map.is_empty());
    }

    #[test]
    fn identity_preprocessor_creates_virtual_entry() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "host = localhost")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "config.toml.identity".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.identity"),
            is_dir: false,
        }];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert!(result.regular_entries.is_empty());
        assert_eq!(result.virtual_entries.len(), 1);

        let virtual_entry = &result.virtual_entries[0];
        assert_eq!(virtual_entry.relative_path, PathBuf::from("config.toml"));
        assert!(!virtual_entry.is_dir);

        // Verify the file was written to the datastore
        let content = env.fs.read_to_string(&virtual_entry.absolute_path).unwrap();
        assert_eq!(content, "host = localhost");

        // Verify source map
        assert_eq!(
            result.source_map[&virtual_entry.absolute_path],
            env.dotfiles_root.join("app/config.toml.identity")
        );
    }

    #[test]
    fn mixed_pack_partitions_correctly() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "host = localhost")
            .file("readme.txt", "hello")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![
            PackEntry {
                relative_path: "config.toml.identity".into(),
                absolute_path: env.dotfiles_root.join("app/config.toml.identity"),
                is_dir: false,
            },
            PackEntry {
                relative_path: "readme.txt".into(),
                absolute_path: env.dotfiles_root.join("app/readme.txt"),
                is_dir: false,
            },
        ];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert_eq!(result.regular_entries.len(), 1);
        assert_eq!(
            result.regular_entries[0].relative_path,
            PathBuf::from("readme.txt")
        );

        assert_eq!(result.virtual_entries.len(), 1);
        assert_eq!(
            result.virtual_entries[0].relative_path,
            PathBuf::from("config.toml")
        );
    }

    #[test]
    fn collision_detection_rejects_conflict() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "preprocessed")
            .file("config.toml", "regular")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![
            PackEntry {
                relative_path: "config.toml.identity".into(),
                absolute_path: env.dotfiles_root.join("app/config.toml.identity"),
                is_dir: false,
            },
            PackEntry {
                relative_path: "config.toml".into(),
                absolute_path: env.dotfiles_root.join("app/config.toml"),
                is_dir: false,
            },
        ];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::PreprocessorCollision { .. }),
            "expected PreprocessorCollision, got: {err}"
        );
    }

    #[test]
    fn merged_entries_combines_and_sorts() {
        let result = PreprocessResult {
            regular_entries: vec![PackEntry {
                relative_path: "zebra".into(),
                absolute_path: "/z".into(),
                is_dir: false,
            }],
            virtual_entries: vec![PackEntry {
                relative_path: "alpha".into(),
                absolute_path: "/a".into(),
                is_dir: false,
            }],
            source_map: HashMap::new(),
            rendered_bytes: HashMap::new(),
            skipped: Vec::new(),
        };

        let merged = result.merged_entries();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].relative_path, PathBuf::from("alpha"));
        assert_eq!(merged[1].relative_path, PathBuf::from("zebra"));
    }

    #[test]
    fn empty_registry_passes_all_through() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "content")
            .done()
            .build();

        let registry = PreprocessorRegistry::new(); // empty!
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "config.toml.identity".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.identity"),
            is_dir: false,
        }];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        // With no preprocessors registered, the file is treated as regular
        assert_eq!(result.regular_entries.len(), 1);
        assert!(result.virtual_entries.is_empty());
    }

    #[test]
    fn directories_are_never_preprocessed() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bin.identity/tool", "#!/bin/sh")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "bin.identity".into(),
            absolute_path: env.dotfiles_root.join("app/bin.identity"),
            is_dir: true, // directory — should NOT be preprocessed
        }];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert_eq!(result.regular_entries.len(), 1);
        assert!(result.virtual_entries.is_empty());
    }

    #[test]
    fn subdirectory_preprocessor_file_preserves_parent() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("subdir/config.toml.identity", "nested content")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "subdir/config.toml.identity".into(),
            absolute_path: env.dotfiles_root.join("app/subdir/config.toml.identity"),
            is_dir: false,
        }];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert_eq!(result.virtual_entries.len(), 1);
        assert_eq!(
            result.virtual_entries[0].relative_path,
            PathBuf::from("subdir/config.toml")
        );
    }

    #[test]
    fn multiple_preprocessor_files_in_one_pack() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "config content")
            .file("settings.json.identity", "settings content")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![
            PackEntry {
                relative_path: "config.toml.identity".into(),
                absolute_path: env.dotfiles_root.join("app/config.toml.identity"),
                is_dir: false,
            },
            PackEntry {
                relative_path: "settings.json.identity".into(),
                absolute_path: env.dotfiles_root.join("app/settings.json.identity"),
                is_dir: false,
            },
        ];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert!(result.regular_entries.is_empty());
        assert_eq!(result.virtual_entries.len(), 2);

        let names: Vec<String> = result
            .virtual_entries
            .iter()
            .map(|e| e.relative_path.to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"config.toml".to_string()));
        assert!(names.contains(&"settings.json".to_string()));

        // Each should have a source_map entry
        assert_eq!(result.source_map.len(), 2);
    }

    #[test]
    fn pack_with_only_preprocessor_files() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("only.conf.identity", "the only file")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "only.conf.identity".into(),
            absolute_path: env.dotfiles_root.join("app/only.conf.identity"),
            is_dir: false,
        }];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert!(result.regular_entries.is_empty());
        assert_eq!(result.virtual_entries.len(), 1);
        assert_eq!(result.merged_entries().len(), 1);
    }

    #[test]
    fn source_map_is_complete() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("a.conf.identity", "aaa")
            .file("b.conf.identity", "bbb")
            .file("regular.txt", "ccc")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![
            PackEntry {
                relative_path: "a.conf.identity".into(),
                absolute_path: env.dotfiles_root.join("app/a.conf.identity"),
                is_dir: false,
            },
            PackEntry {
                relative_path: "b.conf.identity".into(),
                absolute_path: env.dotfiles_root.join("app/b.conf.identity"),
                is_dir: false,
            },
            PackEntry {
                relative_path: "regular.txt".into(),
                absolute_path: env.dotfiles_root.join("app/regular.txt"),
                is_dir: false,
            },
        ];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        // Every virtual entry must have a source_map entry
        for ve in &result.virtual_entries {
            assert!(
                result.source_map.contains_key(&ve.absolute_path),
                "virtual entry {} has no source_map entry",
                ve.absolute_path.display()
            );
        }
        // No regular entries in the source_map
        for re in &result.regular_entries {
            assert!(
                !result.source_map.contains_key(&re.absolute_path),
                "regular entry {} should not be in source_map",
                re.absolute_path.display()
            );
        }
    }

    #[test]
    fn preprocessing_is_idempotent() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "content")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let make_entries = || {
            vec![PackEntry {
                relative_path: "config.toml.identity".into(),
                absolute_path: env.dotfiles_root.join("app/config.toml.identity"),
                is_dir: false,
            }]
        };

        let result1 = preprocess_pack(
            make_entries(),
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();
        let result2 = preprocess_pack(
            make_entries(),
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert_eq!(result1.virtual_entries.len(), result2.virtual_entries.len());
        assert_eq!(
            result1.virtual_entries[0].relative_path,
            result2.virtual_entries[0].relative_path
        );

        // Datastore file should be the same content
        let content1 = env
            .fs
            .read_to_string(&result1.virtual_entries[0].absolute_path)
            .unwrap();
        let content2 = env
            .fs
            .read_to_string(&result2.virtual_entries[0].absolute_path)
            .unwrap();
        assert_eq!(content1, content2);
    }

    #[test]
    fn expansion_error_propagates() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("placeholder", "")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        // Point to a file that doesn't exist — expansion should fail
        let entries = vec![PackEntry {
            relative_path: "missing.conf.identity".into(),
            absolute_path: env.dotfiles_root.join("app/missing.conf.identity"),
            is_dir: false,
        }];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::Fs { .. }),
            "expected Fs error for missing file, got: {err}"
        );
    }

    #[test]
    fn inter_preprocessor_collision_detected() {
        // Two preprocessors produce the same logical name.
        // Set up: `config.toml.identity` and `config.toml.other` (custom
        // extension) both strip to `config.toml`. The pipeline must
        // detect this and refuse rather than silently overwriting.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "a")
            .file("config.toml.other", "b")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(IdentityPreprocessor::new()));
        registry.register(Box::new(IdentityPreprocessor::with_extension("other")));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![
            PackEntry {
                relative_path: "config.toml.identity".into(),
                absolute_path: env.dotfiles_root.join("app/config.toml.identity"),
                is_dir: false,
            },
            PackEntry {
                relative_path: "config.toml.other".into(),
                absolute_path: env.dotfiles_root.join("app/config.toml.other"),
                is_dir: false,
            },
        ];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::PreprocessorCollision { .. }),
            "expected PreprocessorCollision for inter-preprocessor clash, got: {err}"
        );
    }

    #[test]
    fn datastore_preserves_directory_structure() {
        // Preprocessor files in subdirectories should land in matching
        // subdirectories under the datastore, not be flattened with `__`.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("sub/config.toml.identity", "nested")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "sub/config.toml.identity".into(),
            absolute_path: env.dotfiles_root.join("app/sub/config.toml.identity"),
            is_dir: false,
        }];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert_eq!(result.virtual_entries.len(), 1);
        let datastore_path = &result.virtual_entries[0].absolute_path;

        // The datastore path should contain the subdirectory structure, not flattened
        let ds_str = datastore_path.to_string_lossy();
        assert!(
            ds_str.contains("sub/config.toml"),
            "datastore path should preserve directory structure, got: {ds_str}"
        );
        assert!(
            !ds_str.contains("__"),
            "datastore path should not contain flattening separator, got: {ds_str}"
        );

        // File should actually exist at that path
        assert!(env.fs.exists(datastore_path));
        let content = env.fs.read_to_string(datastore_path).unwrap();
        assert_eq!(content, "nested");
    }

    #[test]
    fn datastore_distinguishes_sibling_from_flattened_name() {
        // Regression test for the flatten-with-`__` edge case: a user could
        // have `a/b.txt` and `a__b.txt` both as preprocessor outputs, which
        // would have collided under the old flattening scheme. With
        // directory-preserving storage they live in distinct datastore paths.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("a/b.txt.identity", "nested")
            .file("a__b.txt.identity", "flat")
            .done()
            .build();

        let registry = make_registry();
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![
            PackEntry {
                relative_path: "a/b.txt.identity".into(),
                absolute_path: env.dotfiles_root.join("app/a/b.txt.identity"),
                is_dir: false,
            },
            PackEntry {
                relative_path: "a__b.txt.identity".into(),
                absolute_path: env.dotfiles_root.join("app/a__b.txt.identity"),
                is_dir: false,
            },
        ];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert_eq!(result.virtual_entries.len(), 2);

        // Both files must exist with distinct content
        let nested = result
            .virtual_entries
            .iter()
            .find(|e| e.relative_path == std::path::Path::new("a/b.txt"))
            .expect("nested entry");
        let flat = result
            .virtual_entries
            .iter()
            .find(|e| e.relative_path == std::path::Path::new("a__b.txt"))
            .expect("flat entry");

        assert_ne!(nested.absolute_path, flat.absolute_path);
        assert_eq!(
            env.fs.read_to_string(&nested.absolute_path).unwrap(),
            "nested"
        );
        assert_eq!(env.fs.read_to_string(&flat.absolute_path).unwrap(), "flat");
    }

    // ── Path-traversal defenses ─────────────────────────────────

    /// Test-only preprocessor that emits a configurable set of
    /// [`crate::preprocessing::ExpandedFile`]s — lets tests inject
    /// unsafe paths or directory entries without needing a real archive.
    struct ScriptedPreprocessor {
        name: &'static str,
        extension: &'static str,
        outputs: Vec<crate::preprocessing::ExpandedFile>,
        /// Opt-in flag for tests that exercise the reverse-merge path
        /// (e.g. the conflict-marker safety gate). Off by default so
        /// existing tests of unsafe-path / directory / collision
        /// behaviour aren't accidentally affected by the source-content
        /// scan that the gate adds.
        supports_reverse_merge: bool,
    }

    impl Default for ScriptedPreprocessor {
        fn default() -> Self {
            Self {
                name: "scripted",
                extension: ".scripted",
                outputs: Vec::new(),
                supports_reverse_merge: false,
            }
        }
    }

    impl crate::preprocessing::Preprocessor for ScriptedPreprocessor {
        fn name(&self) -> &str {
            self.name
        }
        fn transform_type(&self) -> crate::preprocessing::TransformType {
            crate::preprocessing::TransformType::Opaque
        }
        fn matches_extension(&self, filename: &str) -> bool {
            filename.ends_with(self.extension)
        }
        fn stripped_name(&self, filename: &str) -> String {
            filename
                .strip_suffix(self.extension)
                .unwrap_or(filename)
                .to_string()
        }
        fn expand(
            &self,
            _source: &Path,
            _fs: &dyn Fs,
        ) -> Result<Vec<crate::preprocessing::ExpandedFile>> {
            Ok(self.outputs.clone())
        }
        fn supports_reverse_merge(&self) -> bool {
            self.supports_reverse_merge
        }
    }

    #[test]
    fn rejects_absolute_path_from_preprocessor() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bad.evil", "x")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "evil",
            extension: ".evil",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("/etc/passwd"),
                content: b"pwn".to_vec(),
                is_dir: false,
                ..Default::default()
            }],
            ..Default::default()
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "bad.evil".into(),
            absolute_path: env.dotfiles_root.join("app/bad.evil"),
            is_dir: false,
        }];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::PreprocessorError { ref message, .. } if message.contains("unsafe path")),
            "expected unsafe-path error, got: {err}"
        );
        // Verify the malicious target was not written
        assert!(!std::path::Path::new("/etc/passwd.dodot-would-have-written-here").exists());
    }

    #[test]
    fn rejects_parent_dir_escape_from_preprocessor() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bad.evil", "x")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "evil",
            extension: ".evil",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("../../escape.txt"),
                content: b"pwn".to_vec(),
                is_dir: false,
                ..Default::default()
            }],
            ..Default::default()
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "bad.evil".into(),
            absolute_path: env.dotfiles_root.join("app/bad.evil"),
            is_dir: false,
        }];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::PreprocessorError { ref message, .. } if message.contains("unsafe path")),
            "expected unsafe-path error, got: {err}"
        );
    }

    #[test]
    fn directory_entry_is_mkdird_not_written_as_file() {
        // A preprocessor emits a directory marker followed by a file
        // inside it. The pipeline must mkdir the directory rather than
        // writing a file at the directory path (which would break the
        // subsequent nested file write).
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bundle.zz", "x")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "scripted",
            extension: ".zz",
            outputs: vec![
                crate::preprocessing::ExpandedFile {
                    relative_path: PathBuf::from("sub"),
                    content: Vec::new(),
                    is_dir: true,
                    ..Default::default()
                },
                crate::preprocessing::ExpandedFile {
                    relative_path: PathBuf::from("sub/nested.txt"),
                    content: b"hello".to_vec(),
                    is_dir: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "bundle.zz".into(),
            absolute_path: env.dotfiles_root.join("app/bundle.zz"),
            is_dir: false,
        }];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert_eq!(result.virtual_entries.len(), 2);

        let dir_entry = result
            .virtual_entries
            .iter()
            .find(|e| e.is_dir)
            .expect("directory entry");
        assert!(
            env.fs.is_dir(&dir_entry.absolute_path),
            "directory entry should be a real directory: {}",
            dir_entry.absolute_path.display()
        );

        let file_entry = result
            .virtual_entries
            .iter()
            .find(|e| !e.is_dir)
            .expect("file entry");
        assert_eq!(
            env.fs.read_to_string(&file_entry.absolute_path).unwrap(),
            "hello"
        );
    }

    #[test]
    fn rejects_empty_path_from_preprocessor() {
        // A preprocessor that produces an empty relative_path (e.g. a
        // template file named literally `.tmpl` whose stripped name is
        // empty) must be rejected with a clean PreprocessorError, not
        // cascaded to the datastore's opaque "empty datastore path"
        // message.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bad.zz", "x")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "scripted",
            extension: ".zz",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from(""),
                content: b"nope".to_vec(),
                is_dir: false,
                ..Default::default()
            }],
            ..Default::default()
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "bad.zz".into(),
            absolute_path: env.dotfiles_root.join("app/bad.zz"),
            is_dir: false,
        }];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::PreprocessorError { ref message, .. } if message.contains("empty output path")),
            "expected empty-path error, got: {err}"
        );
    }

    #[test]
    fn rejects_curdir_only_path_from_preprocessor() {
        // `./` or `.` alone normalises to empty — same rejection.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bad.zz", "x")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "scripted",
            extension: ".zz",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("."),
                content: b"nope".to_vec(),
                is_dir: false,
                ..Default::default()
            }],
            ..Default::default()
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "bad.zz".into(),
            absolute_path: env.dotfiles_root.join("app/bad.zz"),
            is_dir: false,
        }];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::PreprocessorError { ref message, .. } if message.contains("empty output path")),
            "expected empty-path error, got: {err}"
        );
    }

    #[test]
    fn curdir_prefixed_paths_collide_with_plain_paths() {
        // Two preprocessor outputs — one `./foo` and one `foo` — must
        // be treated as a collision. Before normalisation these lived
        // at distinct HashSet keys but the same datastore path, so the
        // second write silently clobbered the first.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bundle.zz", "x")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "scripted",
            extension: ".zz",
            outputs: vec![
                crate::preprocessing::ExpandedFile {
                    relative_path: PathBuf::from("foo"),
                    content: b"first".to_vec(),
                    is_dir: false,
                    ..Default::default()
                },
                crate::preprocessing::ExpandedFile {
                    relative_path: PathBuf::from("./foo"),
                    content: b"second".to_vec(),
                    is_dir: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "bundle.zz".into(),
            absolute_path: env.dotfiles_root.join("app/bundle.zz"),
            is_dir: false,
        }];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::PreprocessorCollision { .. }),
            "expected PreprocessorCollision for ./foo vs foo, got: {err}"
        );
    }

    #[test]
    fn virtual_entry_relative_path_is_normalized() {
        // When a preprocessor emits `./foo`, the resulting virtual entry
        // must carry a normalised relative path. Otherwise downstream
        // code (e.g. rule matching or status display) sees both shapes
        // and treats them as different files.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bundle.zz", "x")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "scripted",
            extension: ".zz",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("./nested/file.txt"),
                content: b"hi".to_vec(),
                is_dir: false,
                ..Default::default()
            }],
            ..Default::default()
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "bundle.zz".into(),
            absolute_path: env.dotfiles_root.join("app/bundle.zz"),
            is_dir: false,
        }];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap();

        assert_eq!(result.virtual_entries.len(), 1);
        assert_eq!(
            result.virtual_entries[0].relative_path,
            PathBuf::from("nested/file.txt"),
            "CurDir components must be stripped from virtual entry"
        );
    }

    // ── Baseline cache integration ──────────────────────────────

    #[test]
    fn baseline_is_written_when_paths_provided_and_tracked_render_present() {
        // End-to-end: a scripted preprocessor that produces a tracked
        // render should result in a baseline JSON on disk under
        // `<cache>/preprocessor/<pack>/preprocessed/<file>.json`. The
        // baseline must round-trip through Baseline::load with all the
        // documented fields populated.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tracked", "name = original")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "tracked-scripted",
            extension: ".tracked",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("config.toml"),
                content: b"name = rendered".to_vec(),
                is_dir: false,
                tracked_render: Some("name = \u{1e}rendered\u{1f}".into()),
                context_hash: Some([0xab; 32]),
            }],
            ..Default::default()
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));

        let entries = vec![PackEntry {
            relative_path: "config.toml.tracked".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
            is_dir: false,
        }];

        preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            false,
        )
        .unwrap();

        let baseline = crate::preprocessing::baseline::Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .expect("baseline must be written for a tracked-render expansion");

        assert_eq!(baseline.rendered_content, "name = rendered");
        assert_eq!(baseline.tracked_render, "name = \u{1e}rendered\u{1f}");
        // Source hash is the SHA of the source file's bytes.
        assert_eq!(baseline.source_hash.len(), 64);
        // Context hash matches the one the preprocessor emitted.
        assert!(
            baseline.context_hash.chars().all(|c| c == 'a' || c == 'b'),
            "context hash should be 0xab repeated, got: {}",
            baseline.context_hash
        );
        assert_eq!(baseline.context_hash.len(), 64);
    }

    #[test]
    fn baseline_is_skipped_in_passive_mode() {
        // Passive callers (`dodot status`, `dodot up --dry-run`) MUST
        // NOT touch the baseline cache. No baseline should be written
        // in that case — overwriting it would erase the
        // divergence-detection ground truth captured at the last
        // `dodot up`. Per `secrets.lex` §7.4 / issue #121.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tracked", "src")
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "tracked-scripted",
            extension: ".tracked",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("config.toml"),
                content: b"x".to_vec(),
                is_dir: false,
                tracked_render: Some("x".into()),
                context_hash: Some([0; 32]),
            }],
            ..Default::default()
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "config.toml.tracked".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
            is_dir: false,
        }];

        preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Passive,
            false,
        )
        .unwrap();

        let path = env
            .paths
            .preprocessor_baseline_path("app", "preprocessed", "config.toml");
        assert!(
            !env.fs.exists(&path),
            "no baseline should exist after a Passive run, but found: {}",
            path.display()
        );
    }

    #[test]
    fn baseline_is_skipped_for_preprocessors_without_tracked_render() {
        // The identity preprocessor (and unarchive) don't produce a
        // tracked render. They still go through the pipeline, but no
        // baseline is written — the cache is only meaningful when paired
        // with burgertocow's marker stream.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "data")
            .done()
            .build();

        let registry = make_registry(); // identity-only
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "config.toml.identity".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.identity"),
            is_dir: false,
        }];

        preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            false,
        )
        .unwrap();

        let path = env
            .paths
            .preprocessor_baseline_path("app", "preprocessed", "config.toml");
        assert!(
            !env.fs.exists(&path),
            "identity preprocessor (no tracked render) should not write a baseline"
        );
    }

    #[test]
    fn baseline_overwrites_on_repeated_up() {
        // Re-running `up` with a changed source file must replace the
        // baseline, not leave the stale one in place — otherwise drift
        // detection would compare against an out-of-date baseline.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tracked", "first")
            .done()
            .build();

        let outputs_first = vec![crate::preprocessing::ExpandedFile {
            relative_path: PathBuf::from("config.toml"),
            content: b"FIRST".to_vec(),
            is_dir: false,
            tracked_render: Some("FIRST".into()),
            context_hash: Some([1; 32]),
        }];
        let outputs_second = vec![crate::preprocessing::ExpandedFile {
            relative_path: PathBuf::from("config.toml"),
            content: b"SECOND".to_vec(),
            is_dir: false,
            tracked_render: Some("SECOND".into()),
            context_hash: Some([2; 32]),
        }];

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let make_entries = || {
            vec![PackEntry {
                relative_path: "config.toml.tracked".into(),
                absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
                is_dir: false,
            }]
        };

        // First run.
        let mut registry1 = PreprocessorRegistry::new();
        registry1.register(Box::new(ScriptedPreprocessor {
            name: "ts",
            extension: ".tracked",
            outputs: outputs_first,
            ..Default::default()
        }));
        preprocess_pack(
            make_entries(),
            &registry1,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            false,
        )
        .unwrap();

        // Second run with changed outputs.
        let mut registry2 = PreprocessorRegistry::new();
        registry2.register(Box::new(ScriptedPreprocessor {
            name: "ts",
            extension: ".tracked",
            outputs: outputs_second,
            ..Default::default()
        }));
        preprocess_pack(
            make_entries(),
            &registry2,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            false,
        )
        .unwrap();

        let baseline = crate::preprocessing::baseline::Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .unwrap();
        assert_eq!(baseline.rendered_content, "SECOND");
    }

    #[test]
    fn end_to_end_baseline_for_real_template_preprocessor() {
        // Exercise the cache write through the actual TemplatePreprocessor
        // (rather than ScriptedPreprocessor). This pins the integration
        // contract: a `.tmpl` file in a pack produces a baseline that
        // contains the rendered content, the tracked render with markers,
        // and a non-empty context hash.
        use std::collections::HashMap;
        let env = TempEnvironment::builder()
            .pack("app")
            .file("greet.tmpl", "hello {{ name }}")
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("name".into(), "Alice".into());
        let template_pp = crate::preprocessing::template::TemplatePreprocessor::new(
            vec!["tmpl".into()],
            vars,
            env.paths.as_ref(),
        )
        .unwrap();
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(template_pp));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "greet.tmpl".into(),
            absolute_path: env.dotfiles_root.join("app/greet.tmpl"),
            is_dir: false,
        }];

        preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            false,
        )
        .unwrap();

        let baseline = crate::preprocessing::baseline::Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "greet",
        )
        .unwrap()
        .expect("template baseline must be written");

        assert_eq!(baseline.rendered_content, "hello Alice");
        // The tracked render must contain marker bytes around "Alice".
        assert!(
            baseline.tracked_render.contains(burgertocow::VAR_START),
            "tracked render must contain marker bytes, got: {:?}",
            baseline.tracked_render
        );
        // Context hash is the template preprocessor's deterministic
        // hex; non-empty.
        assert_eq!(baseline.context_hash.len(), 64);
        // Rendered hash is SHA-256 hex.
        assert_eq!(baseline.rendered_hash.len(), 64);
    }

    // ── Conflict-marker safety gate ─────────────────────────────

    #[test]
    fn conflict_marker_in_template_source_blocks_expansion() {
        // The most important test for R2: a template source containing
        // a dodot-conflict marker must be refused at the pipeline level
        // — otherwise the markers would render verbatim through
        // MiniJinja and deploy into the user's config as garbage.
        use std::collections::HashMap;
        let template_with_conflict = format!(
            "name = Alice\n{}\nhost = \"{{{{ env.DB_HOST }}}}\"\n{}\nhost = \"prod\"\n{}\nport = 5432\n",
            crate::preprocessing::conflict::MARKER_START,
            crate::preprocessing::conflict::MARKER_MID,
            crate::preprocessing::conflict::MARKER_END,
        );
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", &template_with_conflict)
            .done()
            .build();

        let template_pp = crate::preprocessing::template::TemplatePreprocessor::new(
            vec!["tmpl".into()],
            HashMap::new(),
            env.paths.as_ref(),
        )
        .unwrap();
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(template_pp));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "config.toml.tmpl".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.tmpl"),
            is_dir: false,
        }];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            false,
        )
        .unwrap_err();

        match err {
            DodotError::UnresolvedConflictMarker {
                source_file,
                line_numbers,
            } => {
                assert!(source_file.ends_with("config.toml.tmpl"));
                assert_eq!(line_numbers.len(), 3, "got: {line_numbers:?}");
            }
            other => panic!("expected UnresolvedConflictMarker, got: {other}"),
        }

        // Critically: the datastore must NOT carry a partially-rendered
        // file from before the gate caught the markers. The pipeline
        // refuses on the first scan, before any disk write.
        let datastore_path = env
            .paths
            .data_dir()
            .join("packs")
            .join("app")
            .join("preprocessed")
            .join("config.toml");
        assert!(
            !env.fs.exists(&datastore_path),
            "no rendered output should land in the datastore when the gate fires"
        );

        // Same for the baseline cache.
        let baseline_path =
            env.paths
                .preprocessor_baseline_path("app", "preprocessed", "config.toml");
        assert!(
            !env.fs.exists(&baseline_path),
            "no baseline should be written when the gate fires"
        );
    }

    #[test]
    fn conflict_marker_gate_skipped_for_preprocessors_without_reverse_merge() {
        // The unarchive / identity preprocessors don't participate in
        // reverse-merge, so the gate doesn't read their source files
        // (which may not be UTF-8 anyway). Confirm that a marker token
        // accidentally present in such a source does NOT block the
        // pipeline. We use a ScriptedPreprocessor with
        // supports_reverse_merge=false to drive this.
        let env = TempEnvironment::builder()
            .pack("app")
            .file(
                "data.scripted",
                &format!(
                    "header\n{}\nbody\n",
                    crate::preprocessing::conflict::MARKER_START
                ),
            )
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "bytes-only",
            extension: ".scripted",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("data"),
                content: b"emitted".to_vec(),
                is_dir: false,
                ..Default::default()
            }],
            supports_reverse_merge: false,
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "data.scripted".into(),
            absolute_path: env.dotfiles_root.join("app/data.scripted"),
            is_dir: false,
        }];

        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .expect("non-tracking preprocessor must not be gated by markers in its source");
        assert_eq!(result.virtual_entries.len(), 1);
    }

    #[test]
    fn conflict_marker_gate_runs_on_tracking_scripted_preprocessor() {
        // Symmetric to the test above: a ScriptedPreprocessor with
        // supports_reverse_merge=true must trip the gate when its
        // source carries marker lines, even though it's not the real
        // template preprocessor. This pins the gate's dispatch to the
        // trait flag, not a hard-coded preprocessor name check.
        let env = TempEnvironment::builder()
            .pack("app")
            .file(
                "config.toml.tracked",
                &format!(
                    "ok\n{}\nbody\n{}\n",
                    crate::preprocessing::conflict::MARKER_START,
                    crate::preprocessing::conflict::MARKER_END
                ),
            )
            .done()
            .build();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "tracking-bytes",
            extension: ".tracked",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("config.toml"),
                content: b"x".to_vec(),
                is_dir: false,
                tracked_render: Some("x".into()),
                context_hash: Some([0; 32]),
            }],
            supports_reverse_merge: true,
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "config.toml.tracked".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
            is_dir: false,
        }];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::UnresolvedConflictMarker { .. }),
            "expected UnresolvedConflictMarker, got: {err}"
        );
    }

    #[test]
    fn gate_handles_non_utf8_source_via_lossy_decode() {
        // Defence-in-depth: a reverse-merge-capable preprocessor with a
        // non-UTF-8 source must not crash the gate with a generic
        // UTF-8 decode error. The pipeline reads bytes and decodes
        // lossily before scanning for markers — the marker token is
        // ASCII so detection works, and a binary-ish source without
        // markers passes cleanly.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tracked", "placeholder")
            .done()
            .build();

        // Overwrite with non-UTF-8 bytes: a few invalid sequences plus
        // valid ASCII surrounding them. No markers in the bytes.
        let bytes: Vec<u8> = vec![
            b'h', b'e', b'l', b'l', b'o', b'\n', 0xff, 0xfe, b'\n', b'w', b'o', b'r', b'l', b'd',
            b'\n',
        ];
        env.fs
            .write_file(&env.dotfiles_root.join("app/config.toml.tracked"), &bytes)
            .unwrap();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "tracking-bytes",
            extension: ".tracked",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("config.toml"),
                content: b"x".to_vec(),
                is_dir: false,
                tracked_render: Some("x".into()),
                context_hash: Some([0; 32]),
            }],
            supports_reverse_merge: true,
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "config.toml.tracked".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
            is_dir: false,
        }];

        // Should NOT error: the gate's lossy decode handles non-UTF-8
        // gracefully, and there are no marker lines in the bytes.
        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .expect("non-UTF-8 source without markers must not crash the gate");
        assert_eq!(result.virtual_entries.len(), 1);
    }

    #[test]
    fn gate_detects_markers_in_non_utf8_source() {
        // Round-trip the lossy path: a source that's mostly invalid
        // UTF-8 but has a real marker line in valid ASCII still trips
        // the gate. This is the safety-critical scenario — we must
        // not silently pass a marker-bearing source just because
        // surrounding bytes happen to be invalid UTF-8.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tracked", "placeholder")
            .done()
            .build();

        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"prefix\n");
        bytes.push(0xff);
        bytes.push(0xfe);
        bytes.push(b'\n');
        bytes.extend_from_slice(crate::preprocessing::conflict::MARKER_START.as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(b"body\n");
        env.fs
            .write_file(&env.dotfiles_root.join("app/config.toml.tracked"), &bytes)
            .unwrap();

        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(ScriptedPreprocessor {
            name: "tracking-bytes",
            extension: ".tracked",
            outputs: vec![crate::preprocessing::ExpandedFile {
                relative_path: PathBuf::from("config.toml"),
                content: b"x".to_vec(),
                is_dir: false,
                tracked_render: Some("x".into()),
                context_hash: Some([0; 32]),
            }],
            supports_reverse_merge: true,
        }));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "config.toml.tracked".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
            is_dir: false,
        }];

        let err = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, DodotError::UnresolvedConflictMarker { .. }),
            "expected UnresolvedConflictMarker even on non-UTF-8 source, got: {err}"
        );
    }

    #[test]
    fn template_renders_normally_after_markers_are_resolved() {
        // Once the user removes the markers (the standard resolution
        // path), the next `dodot up` must succeed and produce the
        // expected rendered output. This is the round-trip check: the
        // gate doesn't permanently brick a pack — it just defers
        // expansion until the source is clean again.
        use std::collections::HashMap;
        let env = TempEnvironment::builder()
            .pack("app")
            .file("greet.tmpl", "hello {{ name }}")
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("name".into(), "Alice".into());
        let template_pp = crate::preprocessing::template::TemplatePreprocessor::new(
            vec!["tmpl".into()],
            vars,
            env.paths.as_ref(),
        )
        .unwrap();
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(template_pp));

        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "greet.tmpl".into(),
            absolute_path: env.dotfiles_root.join("app/greet.tmpl"),
            is_dir: false,
        }];

        // Round 1: clean source → success.
        let result = preprocess_pack(
            entries.clone(),
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            false,
        )
        .expect("clean source should expand successfully");
        assert_eq!(result.virtual_entries.len(), 1);

        // Round 2: user adds a marker → blocked.
        let dirty = format!(
            "hello\n{}\n{{{{ name }}}}\n{}\n",
            crate::preprocessing::conflict::MARKER_START,
            crate::preprocessing::conflict::MARKER_END,
        );
        env.fs
            .write_file(&env.dotfiles_root.join("app/greet.tmpl"), dirty.as_bytes())
            .unwrap();
        let err = preprocess_pack(
            entries.clone(),
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            false,
        )
        .unwrap_err();
        assert!(matches!(err, DodotError::UnresolvedConflictMarker { .. }));

        // Round 3: user resolves → success again.
        env.fs
            .write_file(
                &env.dotfiles_root.join("app/greet.tmpl"),
                b"hello {{ name }}",
            )
            .unwrap();
        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            false,
        )
        .expect("resolved source should expand again");
        assert_eq!(result.virtual_entries.len(), 1);
    }

    // ── Divergence guard (issue #110, §6.4) ─────────────────────────
    //
    // Tests that `preprocess_pack` refuses to overwrite a deployed file
    // whose bytes have diverged from the cached baseline. The guard
    // reads the file content; env vars are intentionally not part of
    // the staleness signal — see the §6.4 banner and template.rs.
    //
    // Helper that runs the template preprocessor end-to-end. We use the
    // real TemplatePreprocessor here (not ScriptedPreprocessor) so the
    // tests pin the integration contract: a `.tmpl` source produces a
    // baseline that subsequent runs read back.
    fn run_template_preprocess(
        env: &TempEnvironment,
        pack_name: &str,
        force: bool,
    ) -> PreprocessResult {
        use std::collections::HashMap;
        let template_pp = crate::preprocessing::template::TemplatePreprocessor::new(
            vec!["tmpl".into()],
            HashMap::new(),
            env.paths.as_ref(),
        )
        .unwrap();
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(template_pp));

        let datastore = make_datastore(env);
        let pack = make_pack(pack_name, env.dotfiles_root.join(pack_name));
        let entries = vec![PackEntry {
            relative_path: "config.toml.tmpl".into(),
            absolute_path: env.dotfiles_root.join(pack_name).join("config.toml.tmpl"),
            is_dir: false,
        }];

        preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            PreprocessMode::Active,
            force,
        )
        .unwrap()
    }

    #[test]
    fn divergence_guard_skips_when_deployed_was_edited() {
        // Row 3 of the §6.4 matrix: source same, deployed edited.
        // The pipeline must preserve the user's edit (skip the write)
        // and report it via PreprocessResult::skipped.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        // First run: clean deploy, baseline written.
        let first = run_template_preprocess(&env, "app", false);
        assert!(first.skipped.is_empty(), "first deploy must not skip");
        let deployed_path = &first.virtual_entries[0].absolute_path.clone();

        // User edits the deployed file directly.
        env.fs
            .write_file(deployed_path, b"name = USER EDITED")
            .unwrap();

        // Second run with the same source → guard fires.
        let second = run_template_preprocess(&env, "app", false);
        assert_eq!(second.skipped.len(), 1, "deployed-edit must skip");
        let skip = &second.skipped[0];
        assert_eq!(skip.state, DivergenceState::OutputChanged);
        assert_eq!(skip.pack, "app");
        assert_eq!(skip.virtual_relative, std::path::Path::new("config.toml"));

        // The user's edit must still be on disk; the rendered content
        // must NOT have replaced it.
        let on_disk = env.fs.read_to_string(deployed_path).unwrap();
        assert_eq!(on_disk, "name = USER EDITED");

        // The virtual entry must still point at the deployed file so
        // downstream rule matching has something to work with.
        assert_eq!(second.virtual_entries.len(), 1);
        assert_eq!(&second.virtual_entries[0].absolute_path, deployed_path);
    }

    #[test]
    fn divergence_guard_skips_when_both_changed() {
        // Row 4: source AND deployed both edited. Same skip behaviour
        // (preserve deployed bytes), reported as BothChanged so the
        // user gets a sharper warning.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let first = run_template_preprocess(&env, "app", false);
        let deployed_path = first.virtual_entries[0].absolute_path.clone();

        // Edit both the source template and the deployed file.
        env.fs
            .write_file(
                &env.dotfiles_root.join("app/config.toml.tmpl"),
                b"name = SOURCE EDITED",
            )
            .unwrap();
        env.fs
            .write_file(&deployed_path, b"name = USER EDITED")
            .unwrap();

        let second = run_template_preprocess(&env, "app", false);
        assert_eq!(second.skipped.len(), 1);
        assert_eq!(second.skipped[0].state, DivergenceState::BothChanged);

        // Deployed bytes preserved despite the source edit.
        let on_disk = env.fs.read_to_string(&deployed_path).unwrap();
        assert_eq!(on_disk, "name = USER EDITED");
    }

    #[test]
    fn divergence_guard_proceeds_when_source_changed_only() {
        // Row 2: source edited, deployed still matches the cached
        // render. This is the normal "I edited the template, re-deploy"
        // path — the guard must NOT fire here.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let first = run_template_preprocess(&env, "app", false);
        let deployed_path = first.virtual_entries[0].absolute_path.clone();

        // Source edited; deployed left untouched.
        env.fs
            .write_file(
                &env.dotfiles_root.join("app/config.toml.tmpl"),
                b"name = NEW VALUE",
            )
            .unwrap();

        let second = run_template_preprocess(&env, "app", false);
        assert!(
            second.skipped.is_empty(),
            "source-only change must not trigger the guard"
        );
        let on_disk = env.fs.read_to_string(&deployed_path).unwrap();
        assert_eq!(on_disk, "name = NEW VALUE");
    }

    #[test]
    fn divergence_guard_no_op_when_nothing_changed() {
        // Row 1: nothing changed. Re-running deploys the same content;
        // no skip event.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let _ = run_template_preprocess(&env, "app", false);
        let second = run_template_preprocess(&env, "app", false);
        assert!(second.skipped.is_empty());
    }

    #[test]
    fn divergence_guard_overridden_by_force() {
        // `dodot up --force` bypasses the guard: the deployed user edit
        // gets clobbered by the re-rendered output. This is the
        // documented escape hatch (e.g. when an env-var the template
        // references has rotated and the user wants the new value).
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let first = run_template_preprocess(&env, "app", false);
        let deployed_path = first.virtual_entries[0].absolute_path.clone();

        env.fs
            .write_file(&deployed_path, b"name = USER EDITED")
            .unwrap();

        let second = run_template_preprocess(&env, "app", /* force */ true);
        assert!(
            second.skipped.is_empty(),
            "force=true must bypass the guard"
        );
        let on_disk = env.fs.read_to_string(&deployed_path).unwrap();
        assert_eq!(
            on_disk, "name = original",
            "force must rewrite to the rendered content"
        );
    }

    #[test]
    fn divergence_guard_baseline_stays_pinned_to_last_successful_render() {
        // Critical invariant: when the guard skips a write, the
        // baseline must NOT be updated. Otherwise the next
        // `transform check` would compare the user's edit against
        // itself and report Synced — losing the divergence signal.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let first = run_template_preprocess(&env, "app", false);
        let deployed_path = first.virtual_entries[0].absolute_path.clone();

        // Pin the original baseline timestamp/content for comparison.
        let baseline_before = crate::preprocessing::baseline::Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .unwrap();

        env.fs
            .write_file(&deployed_path, b"name = USER EDITED")
            .unwrap();

        let _ = run_template_preprocess(&env, "app", false);

        let baseline_after = crate::preprocessing::baseline::Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            baseline_before.rendered_hash, baseline_after.rendered_hash,
            "baseline must not be rewritten when the guard skips"
        );
        assert_eq!(
            baseline_before.rendered_content, baseline_after.rendered_content,
            "baseline content must not change after a skipped write"
        );
    }

    #[test]
    fn divergence_guard_reproceeds_when_user_undoes_their_edit() {
        // After the guard fires, if the user reverts their edit (or
        // resolves through `dodot transform check`), the next `up`
        // must succeed normally — the guard is not sticky.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let first = run_template_preprocess(&env, "app", false);
        let deployed_path = first.virtual_entries[0].absolute_path.clone();

        // Edit, then revert.
        env.fs
            .write_file(&deployed_path, b"name = USER EDITED")
            .unwrap();
        let blocked = run_template_preprocess(&env, "app", false);
        assert_eq!(blocked.skipped.len(), 1);

        env.fs
            .write_file(&deployed_path, b"name = original")
            .unwrap();
        let cleared = run_template_preprocess(&env, "app", false);
        assert!(
            cleared.skipped.is_empty(),
            "guard must clear once divergence is gone"
        );
    }

    #[test]
    fn divergence_guard_active_for_read_only_callers() {
        // Read-only callers (`dodot status`) set `write_baselines =
        // false` but still need the divergence guard active —
        // otherwise status would silently re-render and overwrite a
        // user's deployed-file edit. This test pins the new behavior:
        // the guard fires regardless of `write_baselines`, and the
        // baseline cache stays pinned to the last `up` (no
        // baseline-write side effects from the read-only call).
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        // Prime the baseline with a normal `up`.
        let _ = run_template_preprocess(&env, "app", false);
        let baseline_before = crate::preprocessing::baseline::Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .unwrap();

        // User edits the deployed file directly.
        let deployed_path = env
            .paths
            .handler_data_dir("app", "preprocessed")
            .join("config.toml");
        env.fs
            .write_file(&deployed_path, b"name = USER EDITED")
            .unwrap();

        // Simulate `status`: write_baselines=false, force=false.
        use std::collections::HashMap;
        let template_pp = crate::preprocessing::template::TemplatePreprocessor::new(
            vec!["tmpl".into()],
            HashMap::new(),
            env.paths.as_ref(),
        )
        .unwrap();
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(template_pp));
        let datastore = make_datastore(&env);
        let pack = make_pack("app", env.dotfiles_root.join("app"));
        let entries = vec![PackEntry {
            relative_path: "config.toml.tmpl".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.tmpl"),
            is_dir: false,
        }];
        let result = preprocess_pack(
            entries,
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            env.paths.as_ref(),
            crate::preprocessing::PreprocessMode::Passive,
            /* force */ false,
        )
        .unwrap();
        assert_eq!(
            result.skipped.len(),
            1,
            "guard must fire for read-only callers too"
        );
        assert_eq!(
            env.fs.read_to_string(&deployed_path).unwrap(),
            "name = USER EDITED",
            "user's deployed-file edit must be preserved"
        );

        // The baseline cache must NOT have been touched: the read-only
        // call leaves the divergence-detection ground truth pinned to
        // the last `up`.
        let baseline_after = crate::preprocessing::baseline::Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .unwrap();
        assert_eq!(baseline_before, baseline_after);
    }
}
