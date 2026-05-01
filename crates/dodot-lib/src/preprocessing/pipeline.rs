//! Preprocessing pipeline — partitions, expands, and merges entries.
//!
//! This module contains the core pipeline function that runs between
//! directory walking and rule matching. It identifies preprocessor files,
//! expands them, writes results to the datastore, checks for collisions,
//! and produces virtual entries for the handler pipeline.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use tracing::{debug, info};

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::packs::Pack;
use crate::paths::Pather;
use crate::preprocessing::baseline::{cache_filename_for, Baseline};
use crate::preprocessing::PreprocessorRegistry;
use crate::rules::PackEntry;
use crate::{DodotError, Result};

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
}

impl PreprocessResult {
    /// Create a passthrough result where all entries are regular (no preprocessing).
    pub fn passthrough(entries: Vec<PackEntry>) -> Self {
        Self {
            regular_entries: entries,
            virtual_entries: Vec::new(),
            source_map: HashMap::new(),
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

/// Run the preprocessing pipeline for a pack's file entries.
///
/// 1. Partition entries into preprocessor files vs regular files.
/// 2. For each preprocessor file: expand, write results to datastore.
/// 3. Create virtual PackEntries pointing to the datastore files.
/// 4. Check for collisions between virtual and regular entries.
/// 5. If `paths` is `Some`, write a baseline-cache record for each
///    expanded file (used by `dodot transform check` and the clean
///    filter to detect drift without re-rendering).
/// 6. Return the result for merging into the handler pipeline.
///
/// Pass `paths = None` from read-only call sites (e.g. `dodot status`)
/// so passive runs don't overwrite a baseline that captured the state
/// of the last `dodot up`.
pub fn preprocess_pack(
    entries: Vec<PackEntry>,
    registry: &PreprocessorRegistry,
    pack: &Pack,
    fs: &dyn Fs,
    datastore: &dyn DataStore,
    paths: Option<&dyn Pather>,
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
        });
    }

    // Phase 2 & 3: Expand and create virtual entries
    let mut virtual_entries = Vec::new();
    let mut source_map = HashMap::new();

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
        // See preprocessing-pipeline.lex §6.3.
        if preprocessor.supports_reverse_merge() {
            let source_str = fs.read_to_string(&entry.absolute_path)?;
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
            let datastore_path = if expanded.is_dir {
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
                "wrote expanded entry"
            );

            // Persist a baseline record so future `dodot transform
            // check` / clean-filter calls can detect drift without
            // re-rendering. Only write when:
            //   - a Pather was provided (read-only callers like
            //     `dodot status` pass None to keep baselines stable),
            //   - the entry is a file (directory entries from archive
            //     preprocessors carry no rendered content), AND
            //   - the preprocessor produced a tracked render (i.e. it's
            //     a generative-with-tracking preprocessor, currently
            //     just templates). Plain Generative preprocessors that
            //     don't support reverse-merge (unarchive) skip the
            //     baseline because the cache is only meaningful when
            //     paired with burgertocow tracking.
            if let (Some(paths), false, Some(tracked)) =
                (paths, expanded.is_dir, expanded.tracked_render.as_deref())
            {
                let cache_filename = cache_filename_for(&virtual_relative);
                let source_bytes = fs.read_file(&entry.absolute_path)?;
                let baseline = Baseline::build(
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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let err = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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
            None,
        )
        .unwrap();
        let result2 = preprocess_pack(
            make_entries(),
            &registry,
            &pack,
            env.fs.as_ref(),
            &datastore,
            None,
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

        let err = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
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

        let err = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let err = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
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

        let err = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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

        let err = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
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

        let err = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
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

        let err = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
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

        let result =
            preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

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
            Some(env.paths.as_ref()),
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
    fn baseline_is_skipped_when_paths_is_none() {
        // Read-only callers (`dodot status`) pass `None`. No baseline
        // should be written in that case — overwriting it would erase
        // the divergence-detection ground truth.
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

        preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None).unwrap();

        let path = env
            .paths
            .preprocessor_baseline_path("app", "preprocessed", "config.toml");
        assert!(
            !env.fs.exists(&path),
            "no baseline should exist after a paths=None run, but found: {}",
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
            Some(env.paths.as_ref()),
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
            Some(env.paths.as_ref()),
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
            Some(env.paths.as_ref()),
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
            Some(env.paths.as_ref()),
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
            Some(env.paths.as_ref()),
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

        let result = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
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

        let err = preprocess_pack(entries, &registry, &pack, env.fs.as_ref(), &datastore, None)
            .unwrap_err();
        assert!(
            matches!(err, DodotError::UnresolvedConflictMarker { .. }),
            "expected UnresolvedConflictMarker, got: {err}"
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
            Some(env.paths.as_ref()),
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
            Some(env.paths.as_ref()),
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
            Some(env.paths.as_ref()),
        )
        .expect("resolved source should expand again");
        assert_eq!(result.virtual_entries.len(), 1);
    }
}
