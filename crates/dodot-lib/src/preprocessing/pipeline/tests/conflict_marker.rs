//! Conflict-marker safety gate: refuse to expand a source file that
//! still carries unresolved <<<<<<< / ======= / >>>>>>> markers.

#![allow(unused_imports)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::datastore::FilesystemDataStore;
use crate::fs::Fs;
use crate::handlers::HandlerConfig;
use crate::packs::Pack;
use crate::paths::Pather;
use crate::preprocessing::pipeline::{preprocess_pack, PreprocessMode, PreprocessorRegistry};
use crate::preprocessing::{ExpandedFile, Preprocessor, TransformType};
use crate::rules::PackEntry;
use crate::testing::TempEnvironment;
use crate::{DodotError, Result};

use super::{make_datastore, make_pack, make_registry, ScriptedPreprocessor};

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
        gate_failure: None,
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
    let baseline_path = env
        .paths
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
            secret_line_ranges: Vec::new(),
            deploy_mode: None,
        }],
        supports_reverse_merge: true,
    }));

    let datastore = make_datastore(&env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));
    let entries = vec![PackEntry {
        relative_path: "config.toml.tracked".into(),
        absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
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
        b'h', b'e', b'l', b'l', b'o', b'\n', 0xff, 0xfe, b'\n', b'w', b'o', b'r', b'l', b'd', b'\n',
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
            secret_line_ranges: Vec::new(),
            deploy_mode: None,
        }],
        supports_reverse_merge: true,
    }));

    let datastore = make_datastore(&env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));
    let entries = vec![PackEntry {
        relative_path: "config.toml.tracked".into(),
        absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
        is_dir: false,
        gate_failure: None,
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
            secret_line_ranges: Vec::new(),
            deploy_mode: None,
        }],
        supports_reverse_merge: true,
    }));

    let datastore = make_datastore(&env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));
    let entries = vec![PackEntry {
        relative_path: "config.toml.tracked".into(),
        absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
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
        gate_failure: None,
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
