//! Baseline-cache integration: the per-render artifact captured by
//! the reverse-merge pipeline (template-only today).

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
            secret_line_ranges: Vec::new(),
            deploy_mode: None,
        }],
        ..Default::default()
    }));

    let datastore = make_datastore(&env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));

    let entries = vec![PackEntry {
        relative_path: "config.toml.tracked".into(),
        absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
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
    assert_eq!(baseline.source_hash.len(), 64);
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
    // `dodot up`. See `secrets.lex` §7.4.
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
            secret_line_ranges: Vec::new(),
            deploy_mode: None,
        }],
        ..Default::default()
    }));

    let datastore = make_datastore(&env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));
    let entries = vec![PackEntry {
        relative_path: "config.toml.tracked".into(),
        absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
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
        gate_failure: None,
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
        secret_line_ranges: Vec::new(),
        deploy_mode: None,
    }];
    let outputs_second = vec![crate::preprocessing::ExpandedFile {
        relative_path: PathBuf::from("config.toml"),
        content: b"SECOND".to_vec(),
        is_dir: false,
        tracked_render: Some("SECOND".into()),
        context_hash: Some([2; 32]),
        secret_line_ranges: Vec::new(),
        deploy_mode: None,
    }];

    let datastore = make_datastore(&env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));
    let make_entries = || {
        vec![PackEntry {
            relative_path: "config.toml.tracked".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.tracked"),
            is_dir: false,
            gate_failure: None,
        }]
    };

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
        gate_failure: None,
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
    assert_eq!(baseline.context_hash.len(), 64);
    assert_eq!(baseline.rendered_hash.len(), 64);
}

// ── Passive: is the cached render still the current one? ─────────

/// The datastore path a passive run keys its virtual entry on — the
/// same key `unrendered` and `rendered_bytes` use.
fn virtual_key(env: &TempEnvironment, pack: &str, filename: &str) -> PathBuf {
    env.paths
        .handler_data_dir(pack, "preprocessed")
        .join(filename)
}

/// Render `greet.tmpl` for real, writing the baseline a later passive
/// run reads. `vars` is the rendering context of that render.
fn render_once(env: &TempEnvironment, vars: &[(&str, &str)]) {
    let registry = template_registry(env, vars);
    let datastore = make_datastore(env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));
    preprocess_pack(
        vec![PackEntry {
            relative_path: "greet.tmpl".into(),
            absolute_path: env.dotfiles_root.join("app/greet.tmpl"),
            is_dir: false,
            gate_failure: None,
        }],
        &registry,
        &pack,
        env.fs.as_ref(),
        &datastore,
        env.paths.as_ref(),
        PreprocessMode::Active,
        false,
    )
    .expect("the active render must succeed");
}

/// Plan the same file passively, the way `status` and adopt's conflict
/// analysis do.
fn plan_passively(
    env: &TempEnvironment,
    vars: &[(&str, &str)],
) -> crate::preprocessing::pipeline::PreprocessResult {
    let registry = template_registry(env, vars);
    let datastore = make_datastore(env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));
    preprocess_pack(
        vec![PackEntry {
            relative_path: "greet.tmpl".into(),
            absolute_path: env.dotfiles_root.join("app/greet.tmpl"),
            is_dir: false,
            gate_failure: None,
        }],
        &registry,
        &pack,
        env.fs.as_ref(),
        &datastore,
        env.paths.as_ref(),
        PreprocessMode::Passive,
        false,
    )
    .expect("the passive plan must succeed")
}

fn template_registry(env: &TempEnvironment, vars: &[(&str, &str)]) -> PreprocessorRegistry {
    let vars: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let mut registry = PreprocessorRegistry::new();
    registry.register(Box::new(
        crate::preprocessing::template::TemplatePreprocessor::new(
            vec!["tmpl".into()],
            vars,
            env.paths.as_ref(),
        )
        .unwrap(),
    ));
    registry
}

#[test]
fn passive_trusts_a_baseline_rendered_from_the_current_source_and_context() {
    // The case the two tests below are measured against: nothing has
    // moved since the render, so the cached content answers for the
    // pack and nothing is listed as awaiting one.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("greet.tmpl", "hello {{ name }}")
        .done()
        .build();

    render_once(&env, &[("name", "Alice")]);
    let result = plan_passively(&env, &[("name", "Alice")]);

    assert!(
        result.unrendered.is_empty(),
        "a current baseline must not be reported as awaiting a render: {:?}",
        result.unrendered
    );
    let key = virtual_key(&env, "app", "greet");
    assert_eq!(
        result.rendered_bytes.get(&key).map(|b| b.to_vec()),
        Some(b"hello Alice".to_vec())
    );
}

#[test]
fn passive_reports_a_template_edited_since_its_render_as_unrendered() {
    // The baseline records `source_hash` precisely so an edited
    // template can be told apart from a current one. A caller that
    // reads a plan as evidence — adopt, deciding whether publishing
    // collides with what another pack claims — must not be handed the
    // previous render's claims as this pack's current ones.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("greet.tmpl", "hello {{ name }}")
        .done()
        .build();

    render_once(&env, &[("name", "Alice")]);
    env.fs
        .write_file(
            &env.dotfiles_root.join("app/greet.tmpl"),
            b"goodbye {{ name }}",
        )
        .unwrap();

    let result = plan_passively(&env, &[("name", "Alice")]);

    let key = virtual_key(&env, "app", "greet");
    assert_eq!(
        result.unrendered,
        vec![key.clone()],
        "an edited template's cached render describes the edit that came before it"
    );
    // Status still needs the previous render: it is what is deployed
    // until the next `dodot up`.
    assert_eq!(
        result.rendered_bytes.get(&key).map(|b| b.to_vec()),
        Some(b"hello Alice".to_vec()),
        "the superseded bytes stay available — they are the deployed file"
    );
}

#[test]
fn passive_reports_a_template_whose_vars_changed_as_unrendered() {
    // Same defect, reached without touching the source: a template's
    // output depends on its rendering context too, and `vars` moving
    // changes what the next `dodot up` writes. `context_hash` is what
    // makes that visible without rendering anything here.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("greet.tmpl", "hello {{ name }}")
        .done()
        .build();

    render_once(&env, &[("name", "Alice")]);
    let result = plan_passively(&env, &[("name", "Bob")]);

    assert_eq!(
        result.unrendered,
        vec![virtual_key(&env, "app", "greet")],
        "the render that produced this baseline used vars this run no longer has"
    );
}

#[test]
fn passive_reports_a_source_it_cannot_read_as_unrendered() {
    // Whether the cached render is current is unanswerable without the
    // source bytes. The answer decides whether adopt may mutate the
    // dotfiles tree, so an unreadable source counts against the cache
    // rather than for it.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("greet.tmpl", "hello {{ name }}")
        .done()
        .build();

    render_once(&env, &[("name", "Alice")]);
    env.fs
        .remove_file(&env.dotfiles_root.join("app/greet.tmpl"))
        .unwrap();

    let result = plan_passively(&env, &[("name", "Alice")]);

    assert_eq!(
        result.unrendered,
        vec![virtual_key(&env, "app", "greet")],
        "a source dodot cannot read is a render it cannot vouch for"
    );
}

#[test]
fn passive_falls_back_to_the_source_check_for_a_preprocessor_without_a_context() {
    // A preprocessor with no rendering context of its own reports
    // `None`, and its baselines carry an empty `context_hash` because
    // there is nothing to hash. That is not a mismatch: the source
    // bytes are the whole of what decides such a render, and reading
    // the empty field as staleness would make every one of these
    // entries refuse forever.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("config.toml.scripted", "src")
        .done()
        .build();

    let scripted = || ScriptedPreprocessor {
        extension: ".scripted",
        outputs: vec![ExpandedFile {
            relative_path: PathBuf::from("config.toml"),
            content: b"rendered".to_vec(),
            is_dir: false,
            tracked_render: Some("rendered".into()),
            context_hash: None,
            secret_line_ranges: Vec::new(),
            deploy_mode: None,
        }],
        supports_reverse_merge: true,
        context_hash: None,
        ..Default::default()
    };

    let datastore = make_datastore(&env);
    let pack = make_pack("app", env.dotfiles_root.join("app"));
    let entries = || {
        vec![PackEntry {
            relative_path: "config.toml.scripted".into(),
            absolute_path: env.dotfiles_root.join("app/config.toml.scripted"),
            is_dir: false,
            gate_failure: None,
        }]
    };
    let mut registry = PreprocessorRegistry::new();
    registry.register(Box::new(scripted()));

    preprocess_pack(
        entries(),
        &registry,
        &pack,
        env.fs.as_ref(),
        &datastore,
        env.paths.as_ref(),
        PreprocessMode::Active,
        false,
    )
    .unwrap();

    let result = preprocess_pack(
        entries(),
        &registry,
        &pack,
        env.fs.as_ref(),
        &datastore,
        env.paths.as_ref(),
        PreprocessMode::Passive,
        false,
    )
    .unwrap();

    assert!(
        result.unrendered.is_empty(),
        "a preprocessor with no context has nothing to compare, and the \
         source bytes are unchanged: {:?}",
        result.unrendered
    );
}

/// A baseline written before `context_hash` existed cannot vouch for a
/// preprocessor that has a context now.
///
/// The template's bytes are untouched and its `vars` have moved, which
/// is precisely what the empty field cannot tell anyone. Passive
/// planning reports the entry rather than serving last render's
/// targets as the current ones — the same conservative answer it gives
/// a source it cannot read, for the same reason: adopt mutates the
/// dotfiles tree on this verdict.
#[test]
fn passive_reports_a_legacy_baseline_as_superseded_when_the_context_moved() {
    let env = TempEnvironment::builder()
        .pack("app")
        .file("greet.tmpl", "hello {{ name }}")
        .done()
        .build();

    render_once(&env, &[("name", "Alice")]);

    // Rewrite the baseline the way an upgrade finds one: every field
    // as this dodot writes it, minus the context hash that did not
    // exist when it was written.
    let path = env
        .paths
        .preprocessor_baseline_path("app", "preprocessed", "greet");
    let raw = env.fs.read_to_string(&path).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    json["context_hash"] = serde_json::Value::String(String::new());
    env.fs
        .write_file(&path, json.to_string().as_bytes())
        .unwrap();

    // The source bytes are what they were; only the vars have moved.
    let result = plan_passively(&env, &[("name", "Bob")]);

    assert_eq!(
        result.unrendered,
        vec![virtual_key(&env, "app", "greet")],
        "a cached render whose context cannot be compared is not one \
         passive planning may vouch for"
    );
}
