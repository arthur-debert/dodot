//! Tests for the preprocessing pipeline.
//!
//! Shared helpers (`make_pack`, `make_registry`, `make_datastore`) and
//! the `ScriptedPreprocessor` test double live here so the per-section
//! sub-modules can `use super::{...}` without re-defining each fixture.
//!
//! General pipeline tests (passthrough, identity expansion, merging,
//! collision detection, partitioning) stay inline. The four big topical
//! suites — path-traversal defenses, baseline-cache integration, the
//! conflict-marker safety gate, and the divergence guard — live in
//! sibling files.

#![allow(unused_imports)]

mod baseline;
mod conflict_marker;
mod divergence;
mod path_traversal;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::datastore::FilesystemDataStore;
use crate::fs::Fs;
use crate::handlers::HandlerConfig;
use crate::packs::Pack;
use crate::paths::Pather;
use crate::preprocessing::identity::IdentityPreprocessor;
use crate::preprocessing::pipeline::{
    preprocess_pack, PreprocessMode, PreprocessResult, PreprocessorRegistry,
};
use crate::preprocessing::{ExpandedFile, Preprocessor, TransformType};
use crate::rules::PackEntry;
use crate::testing::TempEnvironment;
use crate::{DodotError, Result};

pub(super) fn make_pack(name: &str, path: PathBuf) -> Pack {
    Pack::new(name.into(), path, HandlerConfig::default())
}

pub(super) fn make_registry() -> PreprocessorRegistry {
    let mut registry = PreprocessorRegistry::new();
    registry.register(Box::new(IdentityPreprocessor::new()));
    registry
}

pub(super) fn make_datastore(env: &TempEnvironment) -> FilesystemDataStore {
    let runner = Arc::new(crate::datastore::ShellCommandRunner::new(false));
    FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), runner)
}

/// Test-only preprocessor that emits a configurable set of
/// [`crate::preprocessing::ExpandedFile`]s — lets tests inject
/// unsafe paths or directory entries without needing a real archive.
pub(super) struct ScriptedPreprocessor {
    pub(super) name: &'static str,
    pub(super) extension: &'static str,
    pub(super) outputs: Vec<ExpandedFile>,
    /// Opt-in flag for tests that exercise the reverse-merge path
    /// (e.g. the conflict-marker safety gate). Off by default so
    /// existing tests of unsafe-path / directory / collision
    /// behaviour aren't accidentally affected by the source-content
    /// scan that the gate adds.
    pub(super) supports_reverse_merge: bool,
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

impl Preprocessor for ScriptedPreprocessor {
    fn name(&self) -> &str {
        self.name
    }
    fn transform_type(&self) -> TransformType {
        TransformType::Opaque
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
    fn expand(&self, _source: &Path, _fs: &dyn Fs) -> Result<Vec<ExpandedFile>> {
        Ok(self.outputs.clone())
    }
    fn supports_reverse_merge(&self) -> bool {
        self.supports_reverse_merge
    }
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
            gate_failure: None,
        },
        PackEntry {
            relative_path: "gvimrc".into(),
            absolute_path: env.dotfiles_root.join("vim/gvimrc"),
            is_dir: false,
            gate_failure: None,
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
        gate_failure: None,
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
            gate_failure: None,
        },
        PackEntry {
            relative_path: "readme.txt".into(),
            absolute_path: env.dotfiles_root.join("app/readme.txt"),
            is_dir: false,
            gate_failure: None,
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
            gate_failure: None,
        },
        PackEntry {
            relative_path: "config.toml".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml"),
            is_dir: false,
            gate_failure: None,
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
            gate_failure: None,
        }],
        virtual_entries: vec![PackEntry {
            relative_path: "alpha".into(),
            absolute_path: "/a".into(),
            is_dir: false,
            gate_failure: None,
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
        gate_failure: None,
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
        gate_failure: None,
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
        gate_failure: None,
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
            gate_failure: None,
        },
        PackEntry {
            relative_path: "settings.json.identity".into(),
            absolute_path: env.dotfiles_root.join("app/settings.json.identity"),
            is_dir: false,
            gate_failure: None,
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
        gate_failure: None,
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
            gate_failure: None,
        },
        PackEntry {
            relative_path: "b.conf.identity".into(),
            absolute_path: env.dotfiles_root.join("app/b.conf.identity"),
            is_dir: false,
            gate_failure: None,
        },
        PackEntry {
            relative_path: "regular.txt".into(),
            absolute_path: env.dotfiles_root.join("app/regular.txt"),
            is_dir: false,
            gate_failure: None,
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
            gate_failure: None,
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
        gate_failure: None,
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
            gate_failure: None,
        },
        PackEntry {
            relative_path: "config.toml.other".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.other"),
            is_dir: false,
            gate_failure: None,
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
        gate_failure: None,
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
            gate_failure: None,
        },
        PackEntry {
            relative_path: "a__b.txt.identity".into(),
            absolute_path: env.dotfiles_root.join("app/a__b.txt.identity"),
            is_dir: false,
            gate_failure: None,
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
