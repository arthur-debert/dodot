//! Divergence guard (issue #110, §6.4): the "deployed file was edited
//! out-of-band" detector that blocks an automatic re-render.

#![allow(unused_imports)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::datastore::FilesystemDataStore;
use crate::fs::Fs;
use crate::handlers::HandlerConfig;
use crate::packs::Pack;
use crate::paths::Pather;
use crate::preprocessing::divergence::DivergenceState;
use crate::preprocessing::pipeline::{
    preprocess_pack, PreprocessMode, PreprocessResult, PreprocessorRegistry,
};
use crate::preprocessing::{ExpandedFile, Preprocessor, TransformType};
use crate::rules::PackEntry;
use crate::testing::TempEnvironment;
use crate::{DodotError, Result};

use super::{make_datastore, make_pack, make_registry, ScriptedPreprocessor};

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
        gate_failure: None,
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
