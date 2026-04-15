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

// ── genconfig ───────────────────────────────────────────────

#[test]
fn genconfig_returns_template() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);

    let result = commands::genconfig::genconfig(false, &ctx).unwrap();
    assert!(result.content.contains("[pack]"));
    assert!(result.content.contains("[symlink]"));
    assert!(result.content.contains("[mappings]"));
    assert!(result.message.is_none());
}

#[test]
fn genconfig_write_creates_file() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);

    let result = commands::genconfig::genconfig(true, &ctx).unwrap();
    assert!(result.message.is_some());
    env.assert_exists(&env.dotfiles_root.join(".dodot.toml"));
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
