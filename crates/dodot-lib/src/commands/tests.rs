//! Integration tests for command API.

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

struct MockCommandRunner;
impl CommandRunner for MockCommandRunner {
    fn run(&self, _: &str, _: &[String]) -> Result<CommandOutput> {
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

fn make_ctx(env: &TempEnvironment) -> ExecutionContext {
    let runner = Arc::new(MockCommandRunner);
    let datastore = Arc::new(FilesystemDataStore::new(
        env.fs.clone(),
        env.paths.clone(),
        runner,
    ));
    let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());

    ExecutionContext {
        fs: env.fs.clone() as Arc<dyn Fs>,
        datastore,
        paths: env.paths.clone() as Arc<dyn Pather>,
        config_manager,
        dry_run: false,
        no_provision: true,
        provision_rerun: false,
        force: false,
    }
}

// ── status ──────────────────────────────────────────────────

#[test]
fn status_shows_pending_before_up() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    assert_eq!(result.packs.len(), 1);
    assert_eq!(result.packs[0].name, "vim");
    assert!(!result.packs[0].files.is_empty());

    // All should be pending
    for file in &result.packs[0].files {
        assert_eq!(
            file.status, "pending",
            "file {} should be pending",
            file.name
        );
    }
}

#[test]
fn status_renders_with_standout() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    // Render as text
    let output = render::render("pack-status", &result, OutputMode::Text).unwrap();
    assert!(output.contains("vim"), "output: {output}");
    assert!(output.contains("vimrc"), "output: {output}");
    assert!(output.contains("pending"), "output: {output}");

    // Render as JSON
    let json = render::render("pack-status", &result, OutputMode::Json).unwrap();
    assert!(json.contains("\"packs\""), "json: {json}");
}

#[test]
fn status_lists_ignored_packs() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .pack("disabled")
        .file("stuff", "x")
        .ignored()
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    assert_eq!(
        result
            .packs
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["vim"]
    );
    assert_eq!(result.ignored_packs, vec!["disabled".to_string()]);

    let output = render::render("pack-status", &result, OutputMode::Text).unwrap();
    assert!(output.contains("Ignored Packs"), "output: {output}");
    assert!(output.contains("disabled"), "output: {output}");
}

#[test]
fn status_pack_filter_applies_to_ignored_packs() {
    // `dodot status <name>` should narrow both the main listing and the
    // ignored-packs section to just the requested name.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .pack("disabled")
        .file("stuff", "x")
        .ignored()
        .done()
        .pack("old")
        .file("thing", "x")
        .ignored()
        .done()
        .build();

    let ctx = make_ctx(&env);
    let filter = vec!["disabled".to_string()];
    let result = commands::status::status(Some(&filter), &ctx).unwrap();

    assert!(result.packs.is_empty(), "filter should exclude vim");
    assert_eq!(result.ignored_packs, vec!["disabled".to_string()]);
}

// ── status: correct target paths ────────────────────────────

#[test]
fn status_shows_xdg_target_for_subdirectory() {
    // Top-level directories (e.g. `nvim`) are linked wholesale to
    // `$XDG_CONFIG_HOME/<name>` — a single entry in status, not
    // one per nested file.
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("nvim/init.lua", "-- nvim config")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    let nvim_pack = &result.packs[0];
    let nvim_entry = nvim_pack
        .files
        .iter()
        .find(|f| f.name == "nvim")
        .expect("should have nvim dir entry");

    assert!(
        nvim_entry.description.contains(".config/nvim"),
        "expected XDG path for wholesale dir, got: {}",
        nvim_entry.description
    );
}

#[test]
fn status_lists_top_level_dirs_wholesale() {
    // Top-level dirs now appear as single entries (linked wholesale),
    // not expanded into one entry per nested file.
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("nvim/init.lua", "-- nvim config")
        .file("nvim/lua/plugins.lua", "return {}")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    let nvim_pack = &result.packs[0];
    let names: Vec<&str> = nvim_pack.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["nvim"],
        "expected single wholesale dir entry, got {names:?}"
    );
}

// ── up ──────────────────────────────────────────────────────

#[test]
fn up_deploys_packs() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .file("gvimrc", "set guifont=Mono")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();

    assert!(!result.packs.is_empty());
    assert!(result.message.is_some());

    // After up, status should show deployed
    let status = commands::status::status(None, &ctx).unwrap();
    let deployed_count = status.packs[0]
        .files
        .iter()
        .filter(|f| f.status == "deployed")
        .count();
    assert!(deployed_count > 0, "some files should be deployed after up");
}

#[test]
fn up_generates_shell_init() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // Shell init script should exist
    env.assert_exists(&env.paths.init_script_path());
    let init_content = env
        .fs
        .read_to_string(&env.paths.init_script_path())
        .unwrap();
    assert!(
        init_content.contains("aliases.sh"),
        "init script: {init_content}"
    );
}

#[test]
fn up_dry_run_no_changes() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.dry_run = true;

    let result = commands::up::up(None, &ctx).unwrap();
    assert!(result.dry_run);

    // Nothing should be deployed
    let status_ctx = make_ctx(&env); // fresh non-dry-run ctx
    let status = commands::status::status(None, &status_ctx).unwrap();
    for file in &status.packs[0].files {
        assert_eq!(file.status, "pending", "dry run should not deploy");
    }
}

// ── up: conflict handling ──────────────────────────────────

#[test]
fn up_reports_conflict_when_file_exists() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("gitconfig", "[user]\n  name = new")
        .done()
        .home_file(".gitconfig", "[user]\n  name = old")
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();

    // Should report errors
    assert!(
        result.message.as_deref() == Some("Packs deployed with errors."),
        "msg: {:?}",
        result.message
    );

    // The conflict file should show as error
    let error_files: Vec<&commands::DisplayFile> = result.packs[0]
        .files
        .iter()
        .filter(|f| f.status == "error")
        .collect();
    assert!(
        !error_files.is_empty(),
        "should have error files for conflicts"
    );
    assert!(
        error_files[0].status_label.contains("conflict"),
        "should mention conflict: {}",
        error_files[0].status_label
    );

    // Original file should be untouched
    env.assert_file_contents(&env.home.join(".gitconfig"), "[user]\n  name = old");

    // Status should NOT show deployed (no dangling data link)
    let status = commands::status::status(None, &ctx).unwrap();
    for file in &status.packs[0].files {
        assert_eq!(
            file.status, "pending",
            "conflicted file {} should be pending, not deployed",
            file.name
        );
    }
}

#[test]
fn up_force_overwrites_existing_files() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("gitconfig", "[user]\n  name = new")
        .done()
        .home_file(".gitconfig", "[user]\n  name = old")
        .build();

    let mut ctx = make_ctx(&env);
    ctx.force = true;
    let result = commands::up::up(None, &ctx).unwrap();

    // Should succeed
    assert_eq!(result.message.as_deref(), Some("Packs deployed."));

    // File should now be a symlink with new content
    let content = env.fs.read_to_string(&env.home.join(".gitconfig")).unwrap();
    assert_eq!(content, "[user]\n  name = new");
}

// ── down ────────────────────────────────────────────────────

#[test]
fn down_removes_deployed_state() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .build();

    let ctx = make_ctx(&env);

    // Deploy first
    commands::up::up(None, &ctx).unwrap();

    // Verify something is deployed
    let status = commands::status::status(None, &ctx).unwrap();
    let has_deployed = status.packs[0].files.iter().any(|f| f.status == "deployed");
    assert!(has_deployed, "should have deployed files after up");

    // Down
    let down_result = commands::down::down(None, &ctx).unwrap();
    assert!(down_result.message.is_some());

    // All should be pending again
    let status = commands::status::status(None, &ctx).unwrap();
    for file in &status.packs[0].files {
        assert_eq!(
            file.status, "pending",
            "file {} should be pending after down",
            file.name
        );
    }
}

// ── list ────────────────────────────────────────────────────

#[test]
fn list_shows_all_packs() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("gitconfig", "x")
        .done()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .pack("disabled")
        .file("x", "x")
        .ignored()
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::list::list(&ctx).unwrap();

    let names: Vec<&str> = result.packs.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"git"));
    assert!(names.contains(&"vim"));
    assert!(names.contains(&"disabled"));

    let disabled = result.packs.iter().find(|p| p.name == "disabled").unwrap();
    assert!(disabled.ignored);

    // Render as text
    let output = render::render("list", &result, OutputMode::Text).unwrap();
    assert!(output.contains("vim"), "output: {output}");
    assert!(output.contains("(ignored)"), "output: {output}");
}

// ── init ────────────────────────────────────────────────────

#[test]
fn init_creates_pack_directory() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);

    let result = commands::init::init("newpack", &ctx).unwrap();
    assert!(result.message.contains("newpack"));

    env.assert_dir_exists(&env.dotfiles_root.join("newpack"));
    env.assert_exists(&env.dotfiles_root.join("newpack/.dodot.toml"));
}

#[test]
fn init_fails_if_exists() {
    let env = TempEnvironment::builder()
        .pack("existing")
        .file("f", "x")
        .done()
        .build();
    let ctx = make_ctx(&env);

    let err = commands::init::init("existing", &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::PackInvalid { .. }),
        "expected PackInvalid, got: {err}"
    );
}

// ── adopt ───────────────────────────────────────────────────

#[test]
fn adopt_moves_file_and_creates_symlink() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "set nocompatible")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    let result = commands::adopt::adopt("vim", std::slice::from_ref(&source), false, &ctx).unwrap();
    assert!(result.message.contains("1 file"));

    // File should have moved into pack (without dot prefix)
    env.assert_exists(&env.dotfiles_root.join("vim/vimrc"));
    // Symlink should exist at original location
    assert!(env.fs.is_symlink(&source));
}

// ── addignore ───────────────────────────────────────────────

#[test]
fn addignore_creates_file() {
    let env = TempEnvironment::builder()
        .pack("scratch")
        .file("notes", "x")
        .done()
        .build();
    let ctx = make_ctx(&env);

    let result = commands::addignore::addignore("scratch", &ctx).unwrap();
    assert!(result.message.contains("ignored"));
    env.assert_exists(&env.dotfiles_root.join("scratch/.dodotignore"));
}

#[test]
fn addignore_idempotent() {
    let env = TempEnvironment::builder()
        .pack("scratch")
        .file("notes", "x")
        .ignored()
        .done()
        .build();
    let ctx = make_ctx(&env);

    let result = commands::addignore::addignore("scratch", &ctx).unwrap();
    assert!(result.message.contains("already ignored"));
}

// ── nonexistent pack ───────────────────────────────────────

#[test]
fn status_on_nonexistent_pack_returns_error() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let filter = vec!["nonexistent".into()];
    let err = commands::status::status(Some(&filter), &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::PackNotFound { .. }),
        "expected PackNotFound, got: {err}"
    );
}

#[test]
fn up_on_nonexistent_pack_returns_error() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let filter = vec!["typo".into()];
    let err = commands::up::up(Some(&filter), &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::PackNotFound { .. }),
        "expected PackNotFound, got: {err}"
    );
}

// ── down: already down ─────────────────────────────────────

#[test]
fn down_on_already_down_pack_says_nothing_to_do() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .build();

    let ctx = make_ctx(&env);
    // vim was never deployed — should not print misleading output
    let result = commands::down::down(None, &ctx).unwrap();
    assert_eq!(
        result.message.as_deref(),
        Some("Nothing to deactivate."),
        "should say nothing to deactivate"
    );
    assert!(result.packs.is_empty(), "should have no pack entries");
}

// ── addignore: warns about deployed ────────────────────────

#[test]
fn addignore_on_deployed_pack_warns() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("gitconfig", "[user]\n  name = test")
        .done()
        .build();

    let ctx = make_ctx(&env);
    // Deploy first
    commands::up::up(None, &ctx).unwrap();

    // Now addignore should warn
    let result = commands::addignore::addignore("git", &ctx).unwrap();
    assert!(result.message.contains("ignored"));
    let has_warning = result
        .details
        .iter()
        .any(|d| d.contains("currently deployed"));
    assert!(
        has_warning,
        "should warn about deployed pack: {:?}",
        result.details
    );
}

// ── adopt: pack not found hint ─────────────────────────────

#[test]
fn adopt_nonexistent_pack_returns_pack_not_found() {
    let env = TempEnvironment::builder()
        .home_file(".vimrc", "set nocompatible")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err =
        commands::adopt::adopt("newpack", std::slice::from_ref(&source), false, &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::PackNotFound { .. }),
        "expected PackNotFound, got: {err}"
    );
}

// ── full lifecycle ──────────────────────────────────────────

#[test]
fn full_lifecycle_up_status_down_status() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .pack("git")
        .file("gitconfig", "[user]\n  name = test")
        .done()
        .build();

    let ctx = make_ctx(&env);

    // 1. Status before up — all pending
    let s1 = commands::status::status(None, &ctx).unwrap();
    assert_eq!(s1.packs.len(), 2);
    for pack in &s1.packs {
        for file in &pack.files {
            assert_eq!(file.status, "pending");
        }
    }

    // 2. Up — deploy
    let up = commands::up::up(None, &ctx).unwrap();
    assert!(!up.packs.is_empty());

    // 3. Status after up — deployed
    let s2 = commands::status::status(None, &ctx).unwrap();
    let total_deployed: usize = s2
        .packs
        .iter()
        .flat_map(|p| &p.files)
        .filter(|f| f.status == "deployed")
        .count();
    assert!(total_deployed > 0);

    // 4. Down — remove
    commands::down::down(None, &ctx).unwrap();

    // 5. Status after down — pending again
    let s3 = commands::status::status(None, &ctx).unwrap();
    for pack in &s3.packs {
        for file in &pack.files {
            assert_eq!(file.status, "pending");
        }
    }

    // 6. Up again — idempotent
    commands::up::up(None, &ctx).unwrap();
    let s4 = commands::status::status(None, &ctx).unwrap();
    let deployed_again: usize = s4
        .packs
        .iter()
        .flat_map(|p| &p.files)
        .filter(|f| f.status == "deployed")
        .count();
    assert_eq!(total_deployed, deployed_again, "idempotent re-deploy");
}

// ── status: chain verification ────────────────────────────

#[test]
fn status_verified_deployed_after_up() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let result = commands::status::status(None, &ctx).unwrap();
    let file = &result.packs[0].files[0];
    assert_eq!(file.status, "deployed", "should be verified deployed");
    assert_eq!(file.status_label, "deployed");
}

#[test]
fn status_detects_broken_source_deleted() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // Delete the source file — scanner won't find it so pack will have no
    // matches. But the orphaned data link persists in the datastore. This
    // verifies that deleting a source doesn't crash status and that the
    // data link survives (a subsequent `up` would clean it up).
    let source = env.dotfiles_root.join("vim/vimrc");
    env.fs.remove_file(&source).unwrap();

    let result = commands::status::status(None, &ctx).unwrap();
    assert!(
        result.packs[0].files.is_empty(),
        "deleted source should produce no scanner matches"
    );
    assert!(
        env.fs
            .is_symlink(&env.paths.handler_data_dir("vim", "symlink").join("vimrc")),
        "data link should still exist after source deletion"
    );
}

#[test]
fn status_detects_broken_user_link_removed() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // Remove the user link (~/.vimrc)
    let user_path = env.home.join(".vimrc");
    env.fs.remove_file(&user_path).unwrap();

    let result = commands::status::status(None, &ctx).unwrap();
    let file = &result.packs[0].files[0];
    assert_eq!(
        file.status, "stale",
        "should detect missing user link, got: {} ({})",
        file.status, file.status_label
    );
    assert!(
        file.status_label.contains("user link missing"),
        "label: {}",
        file.status_label
    );
}

#[test]
fn status_detects_conflict_at_user_path() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // Replace user symlink with a regular file
    let user_path = env.home.join(".vimrc");
    env.fs.remove_file(&user_path).unwrap();
    env.fs.write_file(&user_path, b"manual file").unwrap();

    let result = commands::status::status(None, &ctx).unwrap();
    let file = &result.packs[0].files[0];
    assert_eq!(
        file.status, "broken",
        "should detect conflict, got: {} ({})",
        file.status, file.status_label
    );
    assert!(
        file.status_label.contains("conflict"),
        "label: {}",
        file.status_label
    );
}

#[test]
fn status_shell_handler_verified_deployed() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let result = commands::status::status(None, &ctx).unwrap();
    let file = result.packs[0]
        .files
        .iter()
        .find(|f| f.handler == "shell")
        .expect("should have shell file");
    assert_eq!(
        file.status, "deployed",
        "shell handler should be verified deployed"
    );
    assert_eq!(file.status_label, "sourced");
}

#[test]
fn status_shell_handler_detects_broken_source() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // Delete source but keep data link
    let source = env.dotfiles_root.join("vim/aliases.sh");
    env.fs.remove_file(&source).unwrap();

    // Scanner won't find the deleted file, so pack will have no matches.
    // Recreate the source so scanner finds it, but break the chain differently.
    // Instead, test that the data link pointing to missing source is detected.
    // We need the file in the pack for the scanner, so write a new one and
    // then break the data link.
    env.fs.write_file(&source, b"alias vi=vim").unwrap();

    // Now manually break the data link by pointing it elsewhere
    let data_link = env
        .paths
        .handler_data_dir("vim", "shell")
        .join("aliases.sh");
    env.fs.remove_file(&data_link).unwrap();
    let bogus = env.dotfiles_root.join("vim/nonexistent");
    env.fs.symlink(&bogus, &data_link).unwrap();

    let result = commands::status::status(None, &ctx).unwrap();
    let file = result.packs[0]
        .files
        .iter()
        .find(|f| f.handler == "shell")
        .expect("should have shell file");
    assert_eq!(
        file.status, "broken",
        "should detect broken data link, got: {} ({})",
        file.status, file.status_label
    );
}

#[test]
fn status_path_handler_verified_deployed() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("bin/myscript", "#!/bin/sh")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let result = commands::status::status(None, &ctx).unwrap();
    let file = result.packs[0]
        .files
        .iter()
        .find(|f| f.handler == "path")
        .expect("should have path file");
    assert_eq!(
        file.status, "deployed",
        "path handler should be verified deployed"
    );
    assert_eq!(file.status_label, "in PATH");
}

// ── cross-pack conflict detection: up command ──────────────

#[test]
fn up_halts_on_cross_pack_symlink_conflict() {
    // Two packs both deploying a file that resolves to ~/.aliases
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("aliases", "alias a=1")
        .done()
        .pack("pack-b")
        .file("aliases", "alias b=2")
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
        .file("aliases", "a")
        .done()
        .pack("conflict-b")
        .file("aliases", "b")
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
        .file("aliases", "a")
        .done()
        .pack("pack-b")
        .file("aliases", "b")
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
    // Both packs use _home/ssh/config → both resolve to ~/.ssh/config
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("_home/ssh/config", "Host a")
        .done()
        .pack("pack-b")
        .file("_home/ssh/config", "Host b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "both targeting ~/.ssh/config should conflict, got: {err}"
    );
}

#[test]
fn up_filtered_packs_only_checks_filtered_subset() {
    // pack-a and pack-b conflict, but if we only deploy pack-a,
    // there's no conflict.
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("aliases", "a")
        .done()
        .pack("pack-b")
        .file("aliases", "b")
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
        .file("aliases", "a")
        .done()
        .pack("b")
        .file("aliases", "b")
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
        .file("aliases", "a")
        .done()
        .pack("beta")
        .file("aliases", "b")
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
        .file("aliases", "a")
        .done()
        .pack("pack-b")
        .file("aliases", "b")
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
        .file("aliases", "a")
        .done()
        .pack("b")
        .file("aliases", "b")
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

// ── edge cases ─────────────────────────────────────────────

#[test]
fn up_succeeds_after_resolving_conflict() {
    // Set up conflicting packs
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("aliases", "a")
        .done()
        .pack("pack-b")
        .file("aliases", "b")
        .done()
        .build();

    let ctx = make_ctx(&env);

    // First attempt fails
    let err = commands::up::up(None, &ctx).unwrap_err();
    assert!(matches!(err, crate::DodotError::CrossPackConflict { .. }));

    // "Resolve" conflict by deploying only one pack
    let filter = vec!["pack-a".into()];
    let result = commands::up::up(Some(&filter), &ctx).unwrap();
    assert_eq!(result.message.as_deref(), Some("Packs deployed."));

    // pack-a should be deployed
    let status = commands::status::status(Some(&filter), &ctx).unwrap();
    assert!(status.packs[0].files.iter().any(|f| f.status == "deployed"));
}

#[test]
fn up_conflict_with_dot_prefix_convention() {
    // pack-a has `dot.bashrc` (uses dot. convention → ~/.bashrc)
    // pack-b has `bashrc` (top-level → ~/.bashrc)
    // Same resolved target → conflict
    let env = TempEnvironment::builder()
        .pack("a")
        .file("dot.bashrc", "# pack a")
        .done()
        .pack("b")
        .file("bashrc", "# pack b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "dot.bashrc and bashrc both resolve to ~/.bashrc: {err}"
    );
}

#[test]
fn up_multiple_simultaneous_conflicts() {
    // Two conflict groups at the same time
    let env = TempEnvironment::builder()
        .pack("a")
        .file("aliases", "a-aliases")
        .file("bashrc", "a-bash")
        .done()
        .pack("b")
        .file("aliases", "b-aliases")
        .file("bashrc", "b-bash")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();

    if let crate::DodotError::CrossPackConflict { conflicts } = &err {
        assert!(
            conflicts.len() >= 2,
            "should detect at least 2 conflict groups, got {}",
            conflicts.len()
        );
    } else {
        panic!("expected CrossPackConflict, got: {err}");
    }
}

#[test]
fn up_ignored_pack_does_not_cause_conflict() {
    // pack-b is ignored, so it shouldn't participate in conflict detection.
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("aliases", "a")
        .done()
        .pack("pack-b")
        .file("aliases", "b")
        .ignored()
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();
    assert_eq!(result.message.as_deref(), Some("Packs deployed."));
}

#[test]
fn status_no_warning_for_same_name_shell_scripts() {
    // Same-name shell scripts in different packs are legitimate
    // and should not produce conflict warnings.
    let env = TempEnvironment::builder()
        .pack("a")
        .file("aliases.sh", "alias a=1")
        .done()
        .pack("b")
        .file("aliases.sh", "alias b=2")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    assert!(
        result.warnings.is_empty(),
        "same-name shell scripts should not produce warnings, got: {:?}",
        result.warnings
    );
}

#[test]
fn up_conflict_xdg_path_both_packs_subdir() {
    // Both packs have nvim/init.lua → both resolve to
    // ~/.config/nvim/init.lua via the default subdirectory rule.
    let env = TempEnvironment::builder()
        .pack("nvim-base")
        .file("nvim/init.lua", "-- base config")
        .done()
        .pack("nvim-custom")
        .file("nvim/init.lua", "-- custom config")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "both targeting ~/.config/nvim/init.lua should conflict: {err}"
    );
}

// ── auto-chmod +x for path handler ─────────────────────────

#[test]
fn up_auto_chmod_makes_bin_files_executable() {
    let env = TempEnvironment::builder()
        .pack("tools")
        .file("bin/deploy", "#!/bin/sh\necho deploying")
        .done()
        .build();

    let ctx = make_ctx(&env);

    // Verify the file starts non-executable
    let tool_path = env.dotfiles_root.join("tools/bin/deploy");
    let meta_before = env.fs.stat(&tool_path).unwrap();
    assert_eq!(meta_before.mode & 0o111, 0, "should start non-executable");

    commands::up::up(None, &ctx).unwrap();

    // After up, file should be executable
    let meta_after = env.fs.stat(&tool_path).unwrap();
    assert_ne!(
        meta_after.mode & 0o111,
        0,
        "bin/ file should be executable after up"
    );
}

#[test]
fn up_auto_chmod_disabled_via_config() {
    let env = TempEnvironment::builder()
        .pack("tools")
        .file("bin/deploy", "#!/bin/sh\necho deploying")
        .done()
        .build();

    // Write root config disabling auto_chmod_exec
    env.fs
        .write_file(
            &env.dotfiles_root.join(".dodot.toml"),
            b"[path]\nauto_chmod_exec = false",
        )
        .unwrap();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // File should remain non-executable
    let tool_path = env.dotfiles_root.join("tools/bin/deploy");
    let meta = env.fs.stat(&tool_path).unwrap();
    assert_eq!(
        meta.mode & 0o111,
        0,
        "auto_chmod_exec=false should leave file non-executable"
    );
}

// ── status: preprocessed file display ──────────────────────────

#[test]
fn status_reports_template_under_stripped_name() {
    // Regression guard: before the fix, status used the raw scanner
    // output (pre-preprocessing) for its file list, so a `greet.tmpl`
    // template would be listed as `greet.tmpl` and wrongly reported as
    // "pending" even after `dodot up` deployed the rendered `greet`.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("greet.tmpl", "hello {{ name }}")
        .config("[preprocessor.template.vars]\nname = \"Alice\"\n")
        .done()
        .build();

    let ctx = make_ctx(&env);

    // Run up first so the deployment state is "deployed".
    commands::up::up(None, &ctx).unwrap();

    let result = commands::status::status(None, &ctx).unwrap();

    assert_eq!(result.packs.len(), 1);
    let files = &result.packs[0].files;
    assert_eq!(files.len(), 1, "files: {files:?}");

    // The display name must be the stripped name, not the .tmpl source.
    assert_eq!(files[0].name, "greet", "file name: {}", files[0].name);
    assert_eq!(
        files[0].status, "deployed",
        "template should report as deployed after up, not pending"
    );
}

#[test]
fn status_reports_template_pending_before_up() {
    // Even without running up, status should use the stripped name.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("greet.tmpl", "hello {{ name }}")
        .config("[preprocessor.template.vars]\nname = \"Alice\"\n")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    let files = &result.packs[0].files;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "greet");
    assert_eq!(files[0].status, "pending");
}
