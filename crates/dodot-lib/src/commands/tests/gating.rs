//! Integration tests for gating behaviour: §7.4 passive-command contract (#121), cross-pack conflict detection, and the C3/C5/gate-before-preprocess regression suite.

#![allow(unused_imports)]

use std::sync::Arc;

use crate::commands;
use crate::config::ConfigManager;
use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
use crate::fs::Fs;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::render;
use crate::testing::TempEnvironment;
use crate::Result;
use standout_render::OutputMode;

use super::support::{make_ctx, make_ctx_with_runner, CannedRunner};

// ── §7.4 passive-command contract (#121) ───────────────────

#[test]
fn status_does_not_write_to_datastore() {
    // §7.4: passive commands MUST NOT mutate the datastore.
    // Running `up` once primes the data dir; running `status`
    // afterwards must leave that state byte-identical.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("config.toml.tmpl", "name = {{ name }}")
        .config("[preprocessor.template.vars]\nname = \"Alice\"\n")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let snapshot = snapshot_dir_contents(&env, env.paths.data_dir());

    // Two consecutive status runs must leave data_dir unchanged.
    commands::status::status(None, &ctx).unwrap();
    commands::status::status(None, &ctx).unwrap();

    let after = snapshot_dir_contents(&env, env.paths.data_dir());
    assert_eq!(
        snapshot, after,
        "status must be byte-identical to the post-up snapshot — \
         no datastore writes allowed"
    );
}

#[test]
fn up_dry_run_does_not_write_to_datastore() {
    // Pin the same §7.4 contract for `up --dry-run`.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("config.toml.tmpl", "name = {{ name }}")
        .config("[preprocessor.template.vars]\nname = \"Alice\"\n")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();
    let snapshot = snapshot_dir_contents(&env, env.paths.data_dir());

    let mut dry_ctx = make_ctx(&env);
    dry_ctx.dry_run = true;
    commands::up::up(None, &dry_ctx).unwrap();

    let after = snapshot_dir_contents(&env, env.paths.data_dir());
    assert_eq!(
        snapshot, after,
        "dry-run must be byte-identical to the post-up snapshot"
    );
}

#[test]
fn install_template_dry_run_emits_correct_sentinel_without_writing_rendered_file() {
    // The §7.4 unblocker: in Passive mode the rendered file isn't
    // on disk, so the install handler used to fail to compute its
    // sentinel. With `rendered_bytes` threaded through, dry-run now
    // emits a Run intent with the same sentinel as the active path
    // would — without ever writing the rendered file. (#121)
    let env = TempEnvironment::builder()
        .pack("app")
        .file("install.sh.tmpl", "#!/bin/sh\necho hello {{ name }}")
        .config("[preprocessor.template.vars]\nname = \"Alice\"\n")
        .done()
        .build();

    // First up establishes the baseline so Passive mode has
    // something to read. no_provision = false so the install
    // handler actually plans Run intents (the default test ctx
    // suppresses code-execution handlers).
    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;
    commands::up::up(None, &ctx).unwrap();

    // Capture the active sentinel (it lives in the executed
    // RunCommand operation).
    let active_intents = crate::packs::orchestration::collect_pack_intents(
        &crate::packs::Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        ),
        &ctx,
    )
    .unwrap();
    let active_sentinel = active_intents
        .iter()
        .find_map(|i| match i {
            crate::operations::HandlerIntent::Run { sentinel, .. } => Some(sentinel.clone()),
            _ => None,
        })
        .expect("active path must produce a Run intent for install.sh");

    // Snapshot the rendered datastore file's existence — Passive
    // must not modify it, but it should already exist from the
    // earlier active up.
    let rendered_path = env
        .paths
        .handler_data_dir("app", "preprocessed")
        .join("install.sh");
    assert!(
        ctx.fs.exists(&rendered_path),
        "active up must have written the rendered file"
    );
    let rendered_before = ctx.fs.read_file(&rendered_path).unwrap();

    // Now plan via dry-run; the install handler must produce the
    // same sentinel from in-memory bytes. Same no_provision = false
    // so the handler actually emits intents.
    let mut dry_ctx = make_ctx(&env);
    dry_ctx.no_provision = false;
    dry_ctx.dry_run = true;
    let plan = crate::packs::orchestration::plan_pack(
        &crate::packs::Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            dry_ctx
                .config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        ),
        &dry_ctx,
        crate::preprocessing::PreprocessMode::Passive,
    )
    .unwrap();
    let dry_sentinel = plan
        .intents
        .iter()
        .find_map(|i| match i {
            crate::operations::HandlerIntent::Run { sentinel, .. } => Some(sentinel.clone()),
            _ => None,
        })
        .expect("passive path must produce a Run intent for install.sh");

    assert_eq!(
        active_sentinel, dry_sentinel,
        "passive sentinel must match active — same rendered bytes either way"
    );
    let rendered_after = ctx.fs.read_file(&rendered_path).unwrap();
    assert_eq!(
        rendered_before, rendered_after,
        "passive must not rewrite the rendered file"
    );
}

#[test]
fn up_dry_run_first_time_pack_with_install_template_does_not_error() {
    // Regression for Copilot review on PR #126: a first-time pack
    // containing `install.sh.tmpl` (no baseline yet, no rendered
    // file on disk) must not crash dry-run intent collection. The
    // install handler used to read `m.absolute_path` unconditionally
    // and propagate an Fs error; now it skips intent generation for
    // the placeholder match instead. Same shape for `Brewfile.tmpl`
    // / homebrew handler.
    let env = TempEnvironment::builder()
        .pack("setup")
        .file("install.sh.tmpl", "#!/bin/sh\necho hello {{ name }}")
        .file("Brewfile.tmpl", "brew '{{ pkg }}'")
        .config("[preprocessor.template.vars]\nname = \"Alice\"\npkg = \"jq\"\n")
        .done()
        .build();

    let mut dry_ctx = make_ctx(&env);
    dry_ctx.no_provision = false;
    dry_ctx.dry_run = true;

    // The fix: this returns Ok and emits zero Run intents (the
    // placeholders skip intent generation cleanly). Pre-fix, the
    // install/homebrew handlers tried to read missing rendered
    // files and propagated an Fs error.
    let result = commands::up::up(None, &dry_ctx).unwrap();
    assert!(result.dry_run);
}

#[test]
fn passive_first_time_pack_surfaces_pending_placeholder() {
    // §7.4 acceptance: a passive command on a brand-new pack with
    // no baseline cache yet must surface a coherent placeholder
    // (template stripped name, status pending), never panic, never
    // fall through to template evaluation. (#121)
    let env = TempEnvironment::builder()
        .pack("app")
        .file("greet.tmpl", "hello {{ name }}")
        .config("[preprocessor.template.vars]\nname = \"Alice\"\n")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    let files = &result.packs[0].files;
    assert_eq!(files.len(), 1, "should surface the templated entry");
    assert_eq!(
        files[0].name, "greet",
        "stripped name (not source filename)"
    );
    assert_eq!(
        files[0].status, "pending",
        "first-time template before any up: pending"
    );
}

/// Snapshot a directory tree (file contents + directory paths) to a
/// stable signature for the §7.4 no-mutation contract tests. Each
/// entry is keyed by absolute path; files map to `Some(contents)`,
/// directories to `None`. Comparing two snapshots for equality is
/// equivalent to "directory tree is byte-identical, including empty
/// directories." Including dir paths catches mutations like
/// `mkdir <data_dir>/<pack>/preprocessed` that would silently slip
/// past a contents-only check.
///
/// Unreadable entries / read errors propagate as panics rather than
/// silent skips — a passive command that produces an unreadable
/// entry under `<data_dir>` is itself a §7.4 violation worth
/// failing the test on.
fn snapshot_dir_contents(
    env: &crate::testing::TempEnvironment,
    root: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, Option<Vec<u8>>> {
    use std::collections::BTreeMap;
    let mut out = BTreeMap::new();
    if !env.fs.exists(root) {
        return out;
    }
    let mut stack: Vec<std::path::PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = env
            .fs
            .read_dir(&dir)
            .unwrap_or_else(|e| panic!("snapshot_dir_contents: read_dir({dir:?}): {e}"));
        for entry in entries {
            if entry.is_dir {
                out.insert(entry.path.clone(), None);
                stack.push(entry.path);
            } else {
                let bytes = env.fs.read_file(&entry.path).unwrap_or_else(|e| {
                    panic!("snapshot_dir_contents: read_file({:?}): {e}", entry.path)
                });
                out.insert(entry.path, Some(bytes));
            }
        }
    }
    out
}

// ── cross-pack conflict detection: up command ──────────────

#[test]
fn up_halts_on_cross_pack_symlink_conflict() {
    // Two packs both deploying a file that resolves to ~/.aliases
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("home.aliases", "alias a=1")
        .done()
        .pack("pack-b")
        .file("home.aliases", "alias b=2")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();

    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "expected CrossPackConflict, got: {err}"
    );

    // Error message should include both packs and the target
    let msg = err.to_string();
    assert!(msg.contains("pack-a"), "msg: {msg}");
    assert!(msg.contains("pack-b"), "msg: {msg}");
    assert!(msg.contains(".aliases"), "msg: {msg}");
}

#[test]
fn up_halts_no_partial_deployment_on_conflict() {
    // When a conflict is detected, NO packs should be deployed —
    // not even the non-conflicting ones.
    let env = TempEnvironment::builder()
        .pack("conflict-a")
        .file("home.aliases", "a")
        .done()
        .pack("conflict-b")
        .file("home.aliases", "b")
        .done()
        .pack("innocent")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let _err = commands::up::up(None, &ctx).unwrap_err();

    // Nothing should be deployed — check the innocent pack
    env.assert_no_handler_state("innocent", "symlink");
    env.assert_no_handler_state("conflict-a", "symlink");
    env.assert_no_handler_state("conflict-b", "symlink");
}

#[test]
fn up_force_does_not_override_cross_pack_conflict() {
    // --force only helps with pre-existing non-dodot files.
    // Cross-pack conflicts are a config problem and --force must NOT help.
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("home.aliases", "a")
        .done()
        .pack("pack-b")
        .file("home.aliases", "b")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.force = true;

    let err = commands::up::up(None, &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "force should NOT override cross-pack conflict, got: {err}"
    );
    assert!(
        err.to_string().contains("--force does not override"),
        "msg: {}",
        err
    );
}

#[test]
fn up_dry_run_still_detects_cross_pack_conflict() {
    let env = TempEnvironment::builder()
        .pack("a")
        .file("bashrc", "a")
        .done()
        .pack("b")
        .file("bashrc", "b")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.dry_run = true;

    let err = commands::up::up(None, &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "dry-run should still detect conflicts, got: {err}"
    );
}

/// Regression: `dodot up` on a cross-pack conflict must render the full
/// per-pack listing, notes, and ignored-pack section — not a bare
/// conflicts dump. Before the fix, the CLI handler hardcoded
/// `packs: Vec::new()` on the `CrossPackConflict` branch, so users only
/// saw the trailing conflicts section and lost all context about what
/// *would* have been deployed.
#[test]
fn up_with_cross_pack_conflict_renders_full_status_view() {
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("home.aliases", "alias a=1")
        .done()
        .pack("pack-b")
        .file("home.aliases", "alias b=2")
        .done()
        // An unrelated pack so we can assert the listing is preserved.
        .pack("innocent")
        .file("home.vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up_or_status_for_conflict(None, &ctx)
        .expect("status fallback should produce Ok on cross-pack conflict");

    // Top-level message explains why nothing deployed.
    assert_eq!(
        result.message.as_deref(),
        Some("Cross-pack conflicts prevent deployment."),
        "got message: {:?}",
        result.message
    );

    // Full per-pack listing is present — the regression was this being empty.
    assert!(
        !result.packs.is_empty(),
        "up-with-conflict must render pack rows, not a bare conflicts dump"
    );
    let pack_names: Vec<&str> = result.packs.iter().map(|p| p.name.as_str()).collect();
    assert!(
        pack_names.contains(&"pack-a"),
        "expected pack-a in listing, got: {:?}",
        pack_names
    );
    assert!(
        pack_names.contains(&"pack-b"),
        "expected pack-b in listing, got: {:?}",
        pack_names
    );
    assert!(
        pack_names.contains(&"innocent"),
        "expected innocent pack in listing, got: {:?}",
        pack_names
    );

    // Conflicts section is still populated — same data the old branch showed.
    assert!(
        !result.conflicts.is_empty(),
        "expected conflicts section to be populated"
    );
    let conflict = &result.conflicts[0];
    assert!(
        conflict.target.contains(".aliases"),
        "conflict target should reference .aliases, got: {}",
        conflict.target
    );

    // Nothing was actually deployed — rows should report pending, not deployed.
    for pack in &result.packs {
        for file in &pack.files {
            assert_ne!(
                file.status, "deployed",
                "{}::{} should not be deployed when conflict blocks up, got: {} ({})",
                pack.name, file.name, file.status, file.status_label
            );
        }
    }
}

#[test]
fn up_no_conflict_when_different_target_files() {
    // Different filenames → different targets → no conflict.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .pack("git")
        .file("gitconfig", "[user]\n  name = test")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();
    assert_eq!(result.message.as_deref(), Some("Packs deployed."));
}

#[test]
fn up_no_conflict_within_same_pack() {
    // Same pack with multiple files targeting different paths — fine.
    let env = TempEnvironment::builder()
        .pack("shell")
        .file("bashrc", "# bash")
        .file("zshrc", "# zsh")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();
    assert_eq!(result.message.as_deref(), Some("Packs deployed."));
}

#[test]
fn up_conflict_via_config_mapping() {
    // Two packs with different source filenames but mapping to the same target
    // via [symlink.targets].
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("settings", "a")
        .config("[symlink]\ntargets = { settings = \"myapp/settings.toml\" }")
        .done()
        .pack("pack-b")
        .file("config", "b")
        .config("[symlink]\ntargets = { config = \"myapp/settings.toml\" }")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();

    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "expected CrossPackConflict for config mapping collision, got: {err}"
    );

    let msg = err.to_string();
    assert!(
        msg.contains("myapp/settings.toml"),
        "should mention the conflicting target: {msg}"
    );
}

#[test]
fn up_conflict_via_home_prefix() {
    // pack-a uses _home/vim/vimrc → ~/.vim/vimrc
    // pack-b uses vim/vimrc (subdirectory) → ~/.config/vim/vimrc
    // These target DIFFERENT paths, so no conflict.
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("_home/vim/vimrc", "a")
        .done()
        .pack("pack-b")
        .file("vim/vimrc", "b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();
    assert_eq!(
        result.message.as_deref(),
        Some("Packs deployed."),
        "different targets should not conflict"
    );
}

#[test]
fn up_conflict_two_packs_same_home_prefix_target() {
    // Both packs use the `home.X` per-file home opt-in → both resolve
    // to ~/.bashrc, conflict.
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("home.bashrc", "# a")
        .done()
        .pack("pack-b")
        .file("home.bashrc", "# b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "both targeting ~/.bashrc should conflict, got: {err}"
    );
}

#[test]
fn up_filtered_packs_only_checks_filtered_subset() {
    // pack-a and pack-b conflict, but if we only deploy pack-a,
    // there's no conflict.
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("home.aliases", "a")
        .done()
        .pack("pack-b")
        .file("home.aliases", "b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let filter = vec!["pack-a".into()];
    let result = commands::up::up(Some(&filter), &ctx).unwrap();

    assert_eq!(result.message.as_deref(), Some("Packs deployed."));
    assert_eq!(result.packs.len(), 1);
    assert_eq!(result.packs[0].name, "pack-a");
}

#[test]
fn up_same_name_shell_scripts_are_not_conflicts() {
    // Two packs both having aliases.sh is a legitimate and common
    // pattern — they're staged in per-pack namespaced directories.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .pack("git")
        .file("aliases.sh", "alias g=git")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();
    assert_eq!(result.message.as_deref(), Some("Packs deployed."));
}

#[test]
fn up_path_dirs_with_different_executables_ok() {
    // Two packs both having bin/ with different file names — no conflict.
    let env = TempEnvironment::builder()
        .pack("tools-a")
        .file("bin/tool-a", "#!/bin/sh")
        .done()
        .pack("tools-b")
        .file("bin/tool-b", "#!/bin/sh")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();
    assert_eq!(result.message.as_deref(), Some("Packs deployed."));
}

#[test]
fn up_path_dirs_with_same_executable_name_conflicts() {
    // Two packs both have bin/tool — one would shadow the other in PATH.
    let env = TempEnvironment::builder()
        .pack("tools-a")
        .file("bin/tool", "#!/bin/sh\necho a")
        .done()
        .pack("tools-b")
        .file("bin/tool", "#!/bin/sh\necho b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "same-name executables across packs should conflict: {err}"
    );
    let msg = err.to_string();
    assert!(msg.contains("tool"), "should mention the executable: {msg}");
    assert!(msg.contains("tools-a"), "should mention pack a: {msg}");
    assert!(msg.contains("tools-b"), "should mention pack b: {msg}");
}

#[test]
fn up_no_cross_handler_conflict() {
    // A shell script and a symlink file with the same name don't conflict
    // because they're in different handler namespaces.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .pack("git")
        .file("gitconfig", "[user]\n  name = test")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();
    assert_eq!(result.message.as_deref(), Some("Packs deployed."));
}

#[test]
fn up_three_packs_partial_conflict() {
    // Three packs, only two conflict — all three are blocked.
    let env = TempEnvironment::builder()
        .pack("a")
        .file("home.aliases", "a")
        .done()
        .pack("b")
        .file("home.aliases", "b")
        .done()
        .pack("c")
        .file("gitconfig", "c")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();

    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "should detect the conflict even if not all packs are involved"
    );

    // Verify nothing was deployed
    env.assert_no_handler_state("a", "symlink");
    env.assert_no_handler_state("b", "symlink");
    env.assert_no_handler_state("c", "symlink");
}

#[test]
fn up_error_message_includes_all_conflict_details() {
    let env = TempEnvironment::builder()
        .pack("alpha")
        .file("home.aliases", "a")
        .done()
        .pack("beta")
        .file("home.aliases", "b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();

    let msg = err.to_string();
    // Should mention both packs
    assert!(msg.contains("alpha"), "msg: {msg}");
    assert!(msg.contains("beta"), "msg: {msg}");
    // Should mention the handler
    assert!(msg.contains("symlink"), "msg: {msg}");
    // Should mention the target path
    assert!(msg.contains(".aliases"), "msg: {msg}");
}

// ── cross-pack conflict detection: status command ──────────

#[test]
fn status_warns_on_potential_cross_pack_conflict() {
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("home.aliases", "a")
        .done()
        .pack("pack-b")
        .file("home.aliases", "b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    // Status should still succeed (it's informational) and surface the
    // conflict as structured data on the result.
    assert_eq!(result.conflicts.len(), 1, "should detect one conflict");
    let c = &result.conflicts[0];
    assert_eq!(c.kind, "symlink");
    let packs: Vec<&str> = c.claimants.iter().map(|cl| cl.pack.as_str()).collect();
    assert!(packs.contains(&"pack-a"), "claimants: {:?}", c.claimants);
    assert!(packs.contains(&"pack-b"), "claimants: {:?}", c.claimants);
}

#[test]
fn status_no_warnings_without_conflicts() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .pack("git")
        .file("gitconfig", "[user]\n  name = test")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    assert!(
        result.warnings.is_empty(),
        "no warnings expected, got: {:?}",
        result.warnings
    );
    assert!(
        result.conflicts.is_empty(),
        "no conflicts expected, got: {:?}",
        result.conflicts
    );
}

#[test]
fn status_shows_conflict_even_when_not_deployed() {
    // Neither pack is deployed yet — status should still show the
    // potential conflict.
    let env = TempEnvironment::builder()
        .pack("a")
        .file("bashrc", "a")
        .done()
        .pack("b")
        .file("bashrc", "b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    // Both packs should show as pending
    for pack in &result.packs {
        for file in &pack.files {
            assert_eq!(file.status, "pending");
        }
    }

    // Conflict data should still be emitted.
    assert!(
        !result.conflicts.is_empty(),
        "should flag potential conflict even when undeployed"
    );
}

#[test]
fn status_filtered_to_one_pack_no_conflict_warning() {
    // If we only ask about one pack, no cross-pack comparison happens.
    let env = TempEnvironment::builder()
        .pack("a")
        .file("home.aliases", "a")
        .done()
        .pack("b")
        .file("home.aliases", "b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let filter = vec!["a".into()];
    let result = commands::status::status(Some(&filter), &ctx).unwrap();

    assert!(
        result.conflicts.is_empty(),
        "single-pack filter should not produce cross-pack conflicts"
    );
}

#[test]
fn status_conflict_with_config_mapping() {
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("settings", "a")
        .config("[symlink]\ntargets = { settings = \"myapp/settings.toml\" }")
        .done()
        .pack("pack-b")
        .file("config", "b")
        .config("[symlink]\ntargets = { config = \"myapp/settings.toml\" }")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    assert_eq!(
        result.conflicts.len(),
        1,
        "config mapping collision should surface one conflict"
    );
    assert!(
        result.conflicts[0].target.contains("settings.toml"),
        "should mention the conflicting target: {:?}",
        result.conflicts[0]
    );
}

// ── C3: pack-level [pack] os ────────────────────────────────────

#[test]
fn pack_os_inactive_pack_surfaces_in_status() {
    // Use an OS value that no real host reports as `dodot.os` so the
    // test is portable across darwin and linux CI.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .pack("mac-only")
        .file("install.sh", "#!/bin/sh\necho mac")
        .config("[pack]\nos = [\"nonexistent-os\"]")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    let active_pack_names: Vec<&str> = result.packs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(active_pack_names, vec!["vim"]);

    assert_eq!(
        result.inactive_packs.len(),
        1,
        "{:?}",
        result.inactive_packs
    );
    let entry = &result.inactive_packs[0];
    assert!(entry.starts_with("mac-only"), "{entry}");
    assert!(entry.contains("os=nonexistent-os"), "{entry}");
    assert!(entry.contains("current="), "{entry}");

    let output = render::render("pack-status", &result, OutputMode::Text).unwrap();
    assert!(output.contains("Inactive on this OS"), "output: {output}");
    assert!(output.contains("mac-only"), "output: {output}");
}

#[test]
fn pack_os_active_pack_runs_normally() {
    // List several OSes including the current target_os values for
    // both darwin and linux so this passes on either host.
    let env = TempEnvironment::builder()
        .pack("portable")
        .file("vimrc", "x")
        .config("[pack]\nos = [\"darwin\", \"linux\", \"windows\"]")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    let active_pack_names: Vec<&str> = result.packs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(active_pack_names, vec!["portable"]);
    assert!(result.inactive_packs.is_empty());
}

#[test]
fn pack_os_macos_alias_matches_darwin_target() {
    // On a darwin host, [pack] os = ["macos"] should match.
    // Skip on non-darwin hosts since we can't fake target_os here.
    if !cfg!(target_os = "macos") {
        return;
    }
    let env = TempEnvironment::builder()
        .pack("mac")
        .file("vimrc", "x")
        .config("[pack]\nos = [\"macos\"]")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();
    let names: Vec<&str> = result.packs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["mac"]);
    assert!(result.inactive_packs.is_empty());
}

#[test]
fn pack_os_inactive_pack_emits_no_operations_in_up() {
    // `dodot up` on an inactive pack should produce a successful, empty
    // PackResult — same shape `.dodotignore` would have if it reached
    // the execute() loop.
    let env = TempEnvironment::builder()
        .pack("mac-only")
        .file("Brewfile", "brew \"ripgrep\"")
        .config("[pack]\nos = [\"nonexistent-os\"]")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();
    // No deployed pack rows.
    assert!(result.packs.is_empty(), "packs: {:?}", result.packs);
}

// ── C5: adopt --only-os ─────────────────────────────────────────

#[test]
fn adopt_only_os_wraps_file_in_gate_dir() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "set nocompatible")
        .build();
    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        Some("darwin"),
        &ctx,
    )
    .unwrap();

    // File is wrapped in `_darwin/` inside the pack — the in-pack
    // path becomes `_darwin/home.vimrc`. On `dodot up`, the gate
    // dir strips on darwin and the `home.X` prefix routes the file
    // to ~/.vimrc.
    env.assert_regular_file(
        &env.dotfiles_root.join("vim/_darwin/home.vimrc"),
        "set nocompatible",
    );
    assert!(env.fs.is_symlink(&source));
}

#[test]
fn adopt_only_os_unknown_label_errors() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "x")
        .build();
    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    let err = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        Some("nonexistent-label"),
        &ctx,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("nonexistent-label"), "missing label: {msg}");
    assert!(msg.contains("--only-os"), "missing flag: {msg}");
}

#[test]
fn adopt_only_os_user_defined_label_works() {
    // A user-defined label in root config is recognised by adopt.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "x")
        .build();
    env.fs
        .write_file(
            &env.dotfiles_root.join(".dodot.toml"),
            b"[gates]\nlaptop = { hostname = \"mbp\" }\n",
        )
        .unwrap();
    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        Some("laptop"),
        &ctx,
    )
    .unwrap();

    env.assert_regular_file(&env.dotfiles_root.join("vim/_laptop/home.vimrc"), "x");
}

// ── Gate-before-preprocess regression ───────────────────────────

#[test]
fn gate_failed_template_does_not_render_at_up() {
    // Regression guard: a gate-failed template (e.g.
    // `aliases._linux.sh.tmpl` on a darwin host) must NOT be expanded by
    // the template preprocessor. If the gate check ran AFTER preprocessing,
    // MiniJinja would render the template and fire secret-provider calls and
    // baseline-cache writes for entries the user explicitly opted out of.
    //
    // Two independent assertions, each catching a distinct failure mode:
    //
    // 1. **Functional**: the template uses `{{ undefined_variable }}`,
    //    a strict-undefined error if MiniJinja runs. If the gate-failed
    //    template reaches the engine, pack planning fails and the
    //    co-located `home.profile` plain file is NOT deployed. Asserting
    //    `~/.profile` exists after `up` proves planning succeeded.
    //
    // 2. **Side-effect**: even if planning happened to succeed somehow,
    //    a render would write a baseline-cache JSON under
    //    `<cache_dir>/preprocessor/p/template/`. Its absence proves the
    //    engine never fired.
    let gated = if cfg!(target_os = "macos") {
        "linux"
    } else if cfg!(target_os = "linux") {
        "darwin"
    } else {
        return; // skip on unsupported hosts
    };
    let template_name = format!("aliases._{gated}.sh.tmpl");
    let env = TempEnvironment::builder()
        .pack("p")
        .file(&template_name, "alias x={{ undefined_variable }}")
        // Co-located plain file. Its deployment proves pack planning
        // didn't abort because of the gated template.
        .file("home.profile", "export PATH=$PATH:~/.local/bin")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // (1) The plain file must be deployed.
    let profile_link = env.home.join(".profile");
    assert!(
        env.fs.exists(&profile_link),
        "co-located plain file was not deployed; pack planning likely failed \
         because the gated template still reached the template engine: {profile_link:?}"
    );

    // (2) No baseline cache for the gated source.
    let baseline_dir = ctx.paths.cache_dir().join("preprocessor/p/template");
    if env.fs.exists(&baseline_dir) {
        let baselines = env.fs.read_dir(&baseline_dir).unwrap_or_default();
        assert!(
            baselines.is_empty(),
            "preprocessor wrote {} baseline file(s) for a gated-out template: {:?}",
            baselines.len(),
            baselines.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
        );
    }

    // No rendered output in the preprocessed dir for the gated template.
    let preprocessed = ctx.paths.data_dir().join("packs/p/preprocessed/aliases.sh");
    assert!(
        !env.fs.exists(&preprocessed),
        "gated-out template was rendered to datastore at {preprocessed:?}"
    );
    // And no shell stage link.
    let shell_link = ctx.paths.data_dir().join("packs/p/shell/aliases.sh");
    assert!(
        !env.fs.exists(&shell_link),
        "gated-out template surfaced as a shell-stage entry at {shell_link:?}"
    );
}

#[test]
fn up_catches_mappings_gates_filename_conflict() {
    // Round-2 review feedback (orchestration.rs:637): the `up` path
    // strips basename gates BEFORE match_entries, so the
    // [mappings.gates] vs filename-gate conflict needs to fire in
    // filter_basename_gates rather than match_entries. Without the
    // fix, this combination would silently pass through `up`.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("install._darwin.sh", "echo x")
        .config("[mappings.gates]\n\"install._darwin.sh\" = \"linux\"\n")
        .done()
        .build();
    let ctx = make_ctx(&env);

    let err = commands::up::up(None, &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("gate-routing conflict"), "msg: {msg}");
    assert!(msg.contains("install._darwin.sh"), "msg: {msg}");
}

#[test]
fn up_rejects_invalid_mappings_gates_glob() {
    // Round-2 review feedback (rules/mod.rs:387): invalid glob
    // patterns in [mappings.gates] used to be silently dropped via
    // `.ok()`. They now hard-error so a typo is loud, not silent.
    let env = TempEnvironment::builder()
        .pack("p")
        .file("vimrc", "x")
        .config("[mappings.gates]\n\"[unclosed\" = \"darwin\"\n")
        .done()
        .build();
    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("invalid `[mappings.gates]` glob"),
        "msg: {msg}"
    );
}

#[test]
fn status_surfaces_gated_template_under_original_name() {
    // Round-3 review feedback (status.rs:545): without pre-preprocess
    // gate filtering in the status path, a gated template would get
    // partitioned by `preprocess_pack` and replaced by a virtual
    // entry whose path is the *stripped* virtual name (e.g.
    // `aliases.sh`), losing the on-disk source name (`aliases._linux.sh.tmpl`).
    // status would then show the virtual name with no indication it
    // was gated.
    let gated = if cfg!(target_os = "macos") {
        "linux"
    } else if cfg!(target_os = "linux") {
        "darwin"
    } else {
        return;
    };
    let template_name = format!("aliases._{gated}.sh.tmpl");
    let env = TempEnvironment::builder()
        .pack("p")
        .file(&template_name, "alias x=y\n")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();
    assert_eq!(result.packs.len(), 1);
    let files = &result.packs[0].files;
    assert_eq!(files.len(), 1, "files: {files:?}");
    let row = &files[0];
    // The status row must surface under the *source* filename so the
    // user can find the file and the gate that dropped it. If
    // preprocessing fired, the row would name the rendered virtual
    // path instead.
    assert_eq!(
        row.name, template_name,
        "expected source filename in row, not a preprocessed virtual name"
    );
    assert_eq!(row.handler, "gate", "row.handler: {}", row.handler);
}

#[test]
fn up_skips_mappings_gated_template() {
    // Round-3 review feedback (orchestration.rs:645): a
    // `[mappings.gates]`-gated template must not reach the
    // preprocessor. Like the basename-gate regression, we use a
    // template that would error if rendered to prove the engine
    // never fired.
    let gated = if cfg!(target_os = "macos") {
        "linux"
    } else if cfg!(target_os = "linux") {
        "darwin"
    } else {
        return;
    };
    let env = TempEnvironment::builder()
        .pack("p")
        .file("aliases.sh.tmpl", "alias x={{ undefined_variable }}")
        .file("home.profile", "export PATH=$PATH:~/.local/bin")
        .config(&format!(
            "[mappings.gates]\n\"aliases.sh.tmpl\" = \"{gated}\"\n"
        ))
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // Plain co-located file deployed → pack planning succeeded.
    let profile_link = env.home.join(".profile");
    assert!(
        env.fs.exists(&profile_link),
        "co-located plain file was not deployed; mapping-gated template \
         likely reached the engine: {profile_link:?}"
    );

    // No baseline cache for the gated template.
    let baseline_dir = ctx.paths.cache_dir().join("preprocessor/p/template");
    if env.fs.exists(&baseline_dir) {
        let baselines = env.fs.read_dir(&baseline_dir).unwrap_or_default();
        assert!(
            baselines.is_empty(),
            "preprocessor wrote {} baseline file(s) for a mapping-gated template: {:?}",
            baselines.len(),
            baselines.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
        );
    }
}

// ── Ignored-pack parity + stale-state sweep (issue #222) ───────────
//
// Two contracts:
//   1. `up`, `down`, and `status` produce the SAME view for an ignored
//      pack — the warning + the "Ignored Packs" section — instead of
//      `up` printing a bare "Packs deployed." with no mention of the
//      pack.
//   2. A pack deployed and *then* marked `.dodotignore` must have its
//      datastore state swept on the next up/down so nothing of it is
//      read/sourced again (the gpg `alias.zsh` regression).

/// Collect the names a result surfaces in its "Ignored Packs" section.
fn ignored_section(result: &commands::PackStatusResult) -> Vec<String> {
    result.ignored_packs.clone()
}

/// True when `warnings` carries the "pack '<name>' is ignored" notice.
fn warns_ignored(result: &commands::PackStatusResult, name: &str) -> bool {
    result
        .warnings
        .iter()
        .any(|w| w.contains(&format!("pack '{name}' is ignored")))
}

#[test]
fn up_down_status_agree_for_explicitly_requested_ignored_pack() {
    // `dodot up gpg` on an ignored pack used to print a generic
    // "Packs deployed." with no warning and no Ignored Packs section,
    // while `dodot status gpg` reported both. All three must agree.
    let env = TempEnvironment::builder()
        .pack("gpg")
        .file("alias.zsh", "alias g='echo hi'")
        .ignored()
        .done()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let filter = vec!["gpg".to_string()];

    let up = commands::up::up(Some(&filter), &ctx).unwrap();
    let down = commands::down::down(Some(&filter), &ctx).unwrap();
    let status = commands::status::status(Some(&filter), &ctx).unwrap();

    for (label, r) in [("up", &up), ("down", &down), ("status", &status)] {
        assert_eq!(
            ignored_section(r),
            vec!["gpg".to_string()],
            "{label} must list gpg in the Ignored Packs section"
        );
        assert!(
            warns_ignored(r, "gpg"),
            "{label} must warn that gpg is ignored"
        );
        // The ignored pack must never appear as a deployable pack row.
        assert!(
            r.packs.iter().all(|p| p.name != "gpg"),
            "{label} must not render gpg as a deployable pack row"
        );
    }
}

#[test]
fn up_lists_ignored_packs_in_full_run() {
    // A bare `dodot up` (no filter) surfaces ignored packs in the same
    // section `status` does — parity for the unfiltered case.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .pack("disabled")
        .file("stuff", "x")
        .ignored()
        .done()
        .build();

    let ctx = make_ctx(&env);
    let up = commands::up::up(None, &ctx).unwrap();
    let status = commands::status::status(None, &ctx).unwrap();

    assert_eq!(ignored_section(&up), vec!["disabled".to_string()]);
    assert_eq!(
        ignored_section(&up),
        ignored_section(&status),
        "up and status must surface the same ignored-pack set"
    );
    // The active pack still deploys normally.
    assert!(up.packs.iter().any(|p| p.name == "vim"));
}

#[test]
fn up_does_not_read_files_from_ignored_pack() {
    // Nothing under a `.dodotignore` pack may be deployed: no symlink,
    // no shell sourcing, no datastore state.
    let env = TempEnvironment::builder()
        .pack("gpg")
        .file("alias.zsh", "alias g='echo hi'")
        .file("gpgrc", "use-agent")
        .ignored()
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // No datastore state for the ignored pack.
    assert!(
        ctx.datastore.list_pack_handlers("gpg").unwrap().is_empty(),
        "ignored pack must leave no datastore state"
    );
    // The init script must not source the ignored pack's shell file.
    let init = env
        .fs
        .read_to_string(&ctx.paths.init_script_path())
        .unwrap_or_default();
    assert!(
        !init.contains("gpg/alias.zsh"),
        "ignored pack's shell file must not be sourced; init script was:\n{init}"
    );
}

#[test]
fn up_sweeps_state_of_pack_ignored_after_deploy() {
    // The gpg regression: deploy a pack, then mark it ignored. The next
    // `up` must tear down its stale datastore state so the regenerated
    // init script stops sourcing it.
    let env = TempEnvironment::builder()
        .pack("gpg")
        .file("alias.zsh", "alias g='echo hi'")
        .done()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);

    // First up: gpg deploys normally and its shell file is sourced.
    commands::up::up(None, &ctx).unwrap();
    assert!(
        !ctx.datastore.list_pack_handlers("gpg").unwrap().is_empty(),
        "precondition: gpg should have state after first up"
    );
    let init_before = env
        .fs
        .read_to_string(&ctx.paths.init_script_path())
        .unwrap();
    assert!(
        init_before.contains("gpg/alias.zsh"),
        "precondition: gpg's shell file should be sourced before ignore"
    );

    // Now mark gpg ignored and run up again.
    env.fs
        .write_file(&env.dotfiles_root.join("gpg/.dodotignore"), b"")
        .unwrap();
    let up = commands::up::up(None, &ctx).unwrap();

    // Stale state is gone …
    assert!(
        ctx.datastore.list_pack_handlers("gpg").unwrap().is_empty(),
        "up must sweep datastore state for a now-ignored pack"
    );
    // … the regenerated init script no longer sources it …
    let init_after = env
        .fs
        .read_to_string(&ctx.paths.init_script_path())
        .unwrap();
    assert!(
        !init_after.contains("gpg/alias.zsh"),
        "init script must stop sourcing a now-ignored pack; was:\n{init_after}"
    );
    // … and it now appears in the Ignored Packs section.
    assert_eq!(ignored_section(&up), vec!["gpg".to_string()]);
}

#[test]
fn down_sweeps_state_of_pack_ignored_after_deploy() {
    // Same sweep contract for `down`.
    let env = TempEnvironment::builder()
        .pack("gpg")
        .file("alias.zsh", "alias g='echo hi'")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();
    assert!(!ctx.datastore.list_pack_handlers("gpg").unwrap().is_empty());

    env.fs
        .write_file(&env.dotfiles_root.join("gpg/.dodotignore"), b"")
        .unwrap();
    let down = commands::down::down(Some(&["gpg".to_string()]), &ctx).unwrap();

    assert!(
        ctx.datastore.list_pack_handlers("gpg").unwrap().is_empty(),
        "down must sweep datastore state for a now-ignored pack"
    );
    let init_after = env
        .fs
        .read_to_string(&ctx.paths.init_script_path())
        .unwrap();
    assert!(!init_after.contains("gpg/alias.zsh"));
    assert_eq!(ignored_section(&down), vec!["gpg".to_string()]);
    // Sweeping leftover state counts as deactivation.
    assert_eq!(down.message.as_deref(), Some("Packs deactivated."));
}

#[test]
fn filtered_up_sweeps_now_ignored_pack_outside_the_filter() {
    // The init script is regenerated from the WHOLE datastore, so a
    // filtered `dodot up vim` must still sweep a now-ignored `gpg` even
    // though gpg isn't in the filter — otherwise gpg keeps being sourced.
    let env = TempEnvironment::builder()
        .pack("gpg")
        .file("alias.zsh", "alias g='echo hi'")
        .done()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();
    assert!(!ctx.datastore.list_pack_handlers("gpg").unwrap().is_empty());

    // Ignore gpg, then run a FILTERED up that names only vim.
    env.fs
        .write_file(&env.dotfiles_root.join("gpg/.dodotignore"), b"")
        .unwrap();
    let up = commands::up::up(Some(&["vim".to_string()]), &ctx).unwrap();

    assert!(
        ctx.datastore.list_pack_handlers("gpg").unwrap().is_empty(),
        "filtered up must still sweep a now-ignored pack outside the filter"
    );
    let init = env
        .fs
        .read_to_string(&ctx.paths.init_script_path())
        .unwrap();
    assert!(
        !init.contains("gpg/alias.zsh"),
        "now-ignored pack must not survive a filtered up; init was:\n{init}"
    );
    // gpg is outside the filter, so it does NOT appear in this run's
    // Ignored Packs section — reporting stays scoped to the request.
    assert!(
        !up.ignored_packs.contains(&"gpg".to_string()),
        "filtered up should not report unrelated ignored packs"
    );
}

#[test]
fn down_dry_run_reports_deactivation_for_now_ignored_pack() {
    // Gemini's dry-run discrepancy: `down --dry-run` must report the
    // same "Packs deactivated." outcome a real run would, when a
    // now-ignored pack still holds stale state.
    let env = TempEnvironment::builder()
        .pack("gpg")
        .file("alias.zsh", "alias g='echo hi'")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();
    env.fs
        .write_file(&env.dotfiles_root.join("gpg/.dodotignore"), b"")
        .unwrap();

    let mut dry_ctx = make_ctx(&env);
    dry_ctx.dry_run = true;
    let down = commands::down::down(None, &dry_ctx).unwrap();

    assert_eq!(
        down.message.as_deref(),
        Some("Packs deactivated."),
        "dry-run must report the same outcome a real run would"
    );
    // Dry-run must NOT actually mutate the datastore.
    assert!(
        !ctx.datastore.list_pack_handlers("gpg").unwrap().is_empty(),
        "down --dry-run must not remove state"
    );
}

#[test]
fn up_mixed_selection_deploys_active_and_reports_ignored() {
    // A mixed `up vim gpg`: vim deploys, gpg is reported ignored — the
    // two outcomes coexist in one result.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .pack("gpg")
        .file("alias.zsh", "alias g='echo hi'")
        .ignored()
        .done()
        .build();

    let ctx = make_ctx(&env);
    let filter = vec!["vim".to_string(), "gpg".to_string()];
    let up = commands::up::up(Some(&filter), &ctx).unwrap();
    let status = commands::status::status(Some(&filter), &ctx).unwrap();

    assert!(
        up.packs.iter().any(|p| p.name == "vim"),
        "vim should deploy in a mixed selection"
    );
    assert_eq!(ignored_section(&up), vec!["gpg".to_string()]);
    assert_eq!(ignored_section(&up), ignored_section(&status));
    assert!(warns_ignored(&up, "gpg"));
}
