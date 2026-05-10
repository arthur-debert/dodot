//! Path-traversal defenses for preprocessor outputs.
//!
//! Pre-flight every `ExpandedFile` against pack-relative path safety:
//! reject absolute paths, parent-dir escapes, empty paths, `./` only,
//! and the `./` collision case.

#![allow(unused_imports)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::datastore::FilesystemDataStore;
use crate::fs::Fs;
use crate::handlers::HandlerConfig;
use crate::packs::Pack;
use crate::paths::Pather;
use crate::preprocessing::pipeline::{
    preprocess_pack, PreprocessMode, PreprocessorRegistry, PREPROCESSED_HANDLER,
};
use crate::preprocessing::{ExpandedFile, Preprocessor, TransformType};
use crate::rules::PackEntry;
use crate::testing::TempEnvironment;
use crate::{DodotError, Result};

use super::{make_datastore, make_pack, make_registry, ScriptedPreprocessor};

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
        matches!(err, DodotError::PreprocessorError { ref message, .. } if message.contains("unsafe path")),
        "expected unsafe-path error, got: {err}"
    );
    // Verify the malicious target was not written
    assert!(!std::path::Path::new("/etc/passwd.dodot-would-have-written-here").exists());
}

#[test]
fn deploy_mode_some_chmods_rendered_file_to_specified_mode() {
    // Pin the §4.3 contract: a preprocessor that emits
    // `deploy_mode = Some(0o600)` (the age / gpg providers do)
    // sees the rendered datastore file land at exactly mode
    // 0600. The default-None case is covered by every other
    // existing pipeline test (templates / unarchive pass
    // through with umask defaults).
    use std::os::unix::fs::PermissionsExt;

    let env = TempEnvironment::builder()
        .pack("app")
        .file("secret.opaque", "src")
        .done()
        .build();

    let mut registry = PreprocessorRegistry::new();
    registry.register(Box::new(ScriptedPreprocessor {
        name: "opaque-with-mode",
        extension: ".opaque",
        outputs: vec![crate::preprocessing::ExpandedFile {
            relative_path: PathBuf::from("secret"),
            content: b"plaintext".to_vec(),
            is_dir: false,
            deploy_mode: Some(0o600),
            ..Default::default()
        }],
        ..Default::default()
    }));

    let datastore = make_datastore(&env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));

    let entries = vec![PackEntry {
        relative_path: "secret.opaque".into(),
        absolute_path: env.dotfiles_root.join("app/secret.opaque"),
        is_dir: false,
        gate_failure: None,
    }];

    preprocess_pack(
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

    // The rendered file lives at the standard preprocessed path.
    let rendered = env
        .paths
        .data_dir()
        .join("packs/app")
        .join(PREPROCESSED_HANDLER)
        .join("secret");
    assert!(rendered.exists(), "rendered file should exist");
    let mode = std::fs::metadata(&rendered).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "deploy_mode = Some(0o600) must produce a 0600 file, got {mode:o}"
    );
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
        PathBuf::from("nested/file.txt"),
        "CurDir components must be stripped from virtual entry"
    );
}
