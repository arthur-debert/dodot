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

/// Regression for #42 (unify status rendering): `up` and `status` must
/// produce identical per-file status_label strings for the same handler
/// state. Before #42, `up` reported "staged bin" while `status` reported
/// "in PATH" for the same path-handler state — confusing duplicate
/// vocabulary.
#[test]
fn up_and_status_produce_matching_labels() {
    let env = TempEnvironment::builder()
        .pack("multi")
        .file("vimrc", "set nocompat") // symlink handler
        .file("aliases.sh", "alias x=y") // shell handler
        .done()
        .pack("withbin")
        .file("bin/tool", "#!/bin/sh\necho hi")
        .done()
        .build();

    let ctx = make_ctx(&env);

    let up_result = commands::up::up(None, &ctx).unwrap();
    let status_result = commands::status::status(None, &ctx).unwrap();

    // Build (pack, file_name) -> status_label maps for both.
    let to_map = |packs: &[commands::DisplayPack]| {
        let mut map = std::collections::HashMap::new();
        for p in packs {
            for f in &p.files {
                if f.status == "error" || f.name.is_empty() {
                    continue; // skip overlay error rows that have no status counterpart
                }
                map.insert((p.name.clone(), f.name.clone()), f.status_label.clone());
            }
        }
        map
    };

    let up_labels = to_map(&up_result.packs);
    let status_labels = to_map(&status_result.packs);

    assert_eq!(
        up_labels, status_labels,
        "up and status should report identical status_labels for the same files"
    );

    // Spot-check the actual labels: should be the steady-state vocabulary
    // ("deployed", "sourced", "in PATH"), not the executor vocabulary
    // ("staged X", "executed: X").
    let labels: Vec<&str> = up_labels.values().map(String::as_str).collect();
    assert!(
        labels.contains(&"in PATH"),
        "expected path handler to render as 'in PATH', got: {labels:?}"
    );
    assert!(
        labels.contains(&"sourced"),
        "expected shell handler to render as 'sourced', got: {labels:?}"
    );
    assert!(
        labels.contains(&"deployed"),
        "expected symlink handler to render as 'deployed', got: {labels:?}"
    );
    assert!(
        labels.iter().all(|l| !l.starts_with("staged ")),
        "no label should use the executor's 'staged X' vocabulary, got: {labels:?}"
    );
}

/// Regression for #42: `down` should likewise render through status, not
/// hand-rolled "removed" / "state removed" labels.
#[test]
fn down_and_status_produce_matching_labels() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .file("aliases.sh", "alias v=vim")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let down_result = commands::down::down(None, &ctx).unwrap();
    let status_result = commands::status::status(None, &ctx).unwrap();

    let to_map = |packs: &[commands::DisplayPack]| {
        let mut map = std::collections::HashMap::new();
        for p in packs {
            for f in &p.files {
                if f.status == "error" || f.name.is_empty() {
                    continue;
                }
                map.insert((p.name.clone(), f.name.clone()), f.status_label.clone());
            }
        }
        map
    };

    let down_labels = to_map(&down_result.packs);
    let status_labels = to_map(&status_result.packs);
    assert_eq!(
        down_labels, status_labels,
        "down and status should report identical status_labels for the same files"
    );

    // After down, files should be in handler-specific pending vocabulary.
    let labels: Vec<&str> = down_labels.values().map(String::as_str).collect();
    assert!(
        labels.iter().all(|l| !l.contains("removed")),
        "down output should use status vocabulary, not 'removed', got: {labels:?}"
    );
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
    // Overlay error rows must identify the failing file in the left column,
    // not render with an empty name (regression: PR #45 review).
    assert!(
        !error_files[0].name.is_empty(),
        "error row should name the failing file, got empty name"
    );
    assert!(
        error_files[0].name.contains("gitconfig"),
        "error row name should reference gitconfig, got: {}",
        error_files[0].name
    );

    // Original file should be untouched
    env.assert_file_contents(&env.home.join(".gitconfig"), "[user]\n  name = old");

    // Status should NOT show deployed. The conflicted file should surface
    // as `warning` (PendingConflict) with a footnote pointing at the
    // pre-existing user file — see #43.
    let status = commands::status::status(None, &ctx).unwrap();
    for file in &status.packs[0].files {
        assert!(
            matches!(file.status.as_str(), "pending" | "warning"),
            "conflicted file {} should be pending or warning, got {}",
            file.name,
            file.status
        );
    }
    let conflicted = status.packs[0]
        .files
        .iter()
        .find(|f| f.status == "warning")
        .expect("the conflicted file should surface as warning (PendingConflict)");
    assert!(
        conflicted.status_label.starts_with("pending ("),
        "warning label should keep 'pending' and add footnote ref, got: {}",
        conflicted.status_label
    );
    assert!(
        !status.packs[0].footnotes.is_empty(),
        "pack should have a footnote describing the pre-existing file"
    );
    assert!(
        status.packs[0].footnotes[0].contains(".gitconfig"),
        "footnote should mention the conflicting path, got: {}",
        status.packs[0].footnotes[0]
    );
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

    // After down, all files should be plain pending. The user-side
    // symlinks left dangling by `down` are NOT conflicts — the executor's
    // create_user_link gracefully replaces them on the next `up`. (#43
    // refines `PendingConflict` to only fire when the executor would
    // actually refuse: non-symlink + exists.)
    let status = commands::status::status(None, &ctx).unwrap();
    for file in &status.packs[0].files {
        assert_eq!(
            file.status, "pending",
            "file {} should be pending after down (dangling symlinks are not conflicts), got {}",
            file.name, file.status
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

    let result = commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // File should have moved into pack (without dot prefix), content preserved
    env.assert_regular_file(&env.dotfiles_root.join("vim/vimrc"), "set nocompatible");
    // Symlink should exist at original location
    assert!(env.fs.is_symlink(&source));

    // Status output should include the vim pack with the adopted file
    assert!(result.packs.iter().any(|p| p.name == "vim"));
    let vim = result.packs.iter().find(|p| p.name == "vim").unwrap();
    assert!(vim.files.iter().any(|f| f.name == "vimrc"));
}

#[test]
fn adopt_preserves_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let env = TempEnvironment::builder()
        .pack("tools")
        .file("placeholder", "")
        .done()
        .home_file("script.sh", "#!/bin/sh\necho hi")
        .build();

    let source = env.home.join("script.sh");
    // Mark source as executable
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&source, perms).unwrap();

    let ctx = make_ctx(&env);
    commands::adopt::adopt(
        "tools",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    let dest = env.dotfiles_root.join("tools/script.sh");
    let meta = std::fs::metadata(&dest).unwrap();
    assert_eq!(
        meta.permissions().mode() & 0o777,
        0o755,
        "executable bit should be preserved on adopted file"
    );
}

#[test]
fn adopt_destination_conflict_refused_without_force() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "existing content")
        .done()
        .home_file(".vimrc", "new content")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    let err = commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();
    assert!(
        matches!(err, crate::DodotError::SymlinkConflict { .. }),
        "expected SymlinkConflict, got: {err}"
    );

    // Original file untouched; existing pack file untouched.
    env.assert_regular_file(&source, "new content");
    env.assert_regular_file(&env.dotfiles_root.join("vim/vimrc"), "existing content");
}

#[test]
fn adopt_destination_conflict_resolved_with_force() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "OLD")
        .done()
        .home_file(".vimrc", "NEW")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        true, // --force
        false,
        false,
        &ctx,
    )
    .unwrap();

    env.assert_regular_file(&env.dotfiles_root.join("vim/vimrc"), "NEW");
    assert!(env.fs.is_symlink(&source));
}

#[test]
fn adopt_directory_creates_symlink_and_preserves_contents() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("placeholder", "")
        .done()
        .home_file(".config/nvim/init.lua", "-- config")
        .home_file(".config/nvim/lua/mod.lua", "-- module")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config");

    commands::adopt::adopt(
        "nvim",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // The directory is moved to pack/config (dot stripped) with contents.
    let pack_dir = env.dotfiles_root.join("nvim/config");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("nvim/init.lua"), "-- config");
    env.assert_regular_file(&pack_dir.join("nvim/lua/mod.lua"), "-- module");

    // Original path is now a symlink to the pack copy.
    assert!(env.fs.is_symlink(&source));
    let target = env.fs.readlink(&source).unwrap();
    assert_eq!(target, pack_dir);
}

#[test]
fn adopt_preserves_inner_symlinks_as_symlinks() {
    let env = TempEnvironment::builder()
        .pack("shell")
        .file("placeholder", "")
        .done()
        .home_file("mydir/real.txt", "hello")
        .build();

    // Create an inner symlink: mydir/alias -> mydir/real.txt
    let inner_target = env.home.join("mydir/real.txt");
    let inner_link = env.home.join("mydir/alias");
    env.fs.symlink(&inner_target, &inner_link).unwrap();

    let ctx = make_ctx(&env);
    let source = env.home.join("mydir");
    commands::adopt::adopt(
        "shell",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // The inner link should still be a symlink inside the pack copy.
    let copied_link = env.dotfiles_root.join("shell/mydir/alias");
    assert!(
        env.fs.is_symlink(&copied_link),
        "inner symlink should be preserved as a symlink, not followed"
    );
}

#[test]
fn adopt_nested_source_refused() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("placeholder", "")
        .done()
        .home_file(".config/nvim/init.lua", "-- config")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/nvim/init.lua");

    let err = commands::adopt::adopt(
        "nvim",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("nested"),
        "expected 'nested' in error message, got: {msg}"
    );

    // Nothing mutated.
    env.assert_regular_file(&source, "-- config");
    env.assert_not_exists(&env.dotfiles_root.join("nvim/init.lua"));
}

#[test]
fn adopt_already_adopted_source_is_skipped() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "content")
        .done()
        .build();

    // Pre-link home file to the pack.
    let source = env.home.join(".vimrc");
    let pack_file = env.dotfiles_root.join("vim/vimrc");
    env.fs.symlink(&pack_file, &source).unwrap();

    let ctx = make_ctx(&env);
    let result = commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("already managed")),
        "expected already-managed warning, got: {:?}",
        result.warnings
    );
    // Source still a symlink, pack file untouched.
    assert!(env.fs.is_symlink(&source));
    env.assert_regular_file(&pack_file, "content");
}

#[test]
fn adopt_relative_path_with_curdir_normalizes() {
    // `dodot adopt mypack ./.vimrc` run from HOME must not be rejected as
    // "nested" — the `.` component should normalize away so parent == HOME.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "content")
        .build();

    // Run with CWD = HOME so the relative path resolves naturally.
    let prev_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&env.home).unwrap();
    let ctx = make_ctx(&env);
    let result = commands::adopt::adopt(
        "vim",
        &[std::path::PathBuf::from("./.vimrc")],
        false,
        false,
        false,
        &ctx,
    );
    std::env::set_current_dir(prev_cwd).unwrap();

    result.expect("adopt should accept ./.vimrc when CWD is HOME");
    env.assert_regular_file(&env.dotfiles_root.join("vim/vimrc"), "content");
    assert!(env.fs.is_symlink(&env.home.join(".vimrc")));
}

#[test]
fn adopt_ignored_pack_refused() {
    let env = TempEnvironment::builder()
        .pack("disabled")
        .file("placeholder", "")
        .ignored()
        .done()
        .home_file(".vimrc", "x")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        "disabled",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();
    assert!(
        matches!(err, crate::DodotError::PackInvalid { .. }),
        "expected PackInvalid, got: {err}"
    );
}

#[test]
fn adopt_filename_matching_pack_ignore_refused() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .config("[pack]\nignore = [\"*.bak\"]")
        .done()
        .home_file(".vimrc.bak", "old")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc.bak");
    let err = commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("ignore"),
        "expected ignore-pattern message, got: {msg}"
    );
}

#[test]
fn adopt_broken_pack_blocks_conflict_check() {
    // If another pack fails intent collection, adoption must refuse rather
    // than silently proceed — otherwise the conflict check produces a false
    // negative and we'd mutate into a state `dodot up` would later reject.
    let env = TempEnvironment::builder()
        .pack("broken")
        .file("config.toml.tmpl", "{{ missing_var }}")
        .done()
        .pack("target")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "content")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        "target",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();

    // The error surfaces from the broken pack's intent collection
    // (template render failure), not a silent success.
    assert!(
        matches!(err, crate::DodotError::TemplateRender { .. }),
        "expected the broken pack's error to surface, got: {err}"
    );

    // Home untouched; no pack copy left behind.
    env.assert_regular_file(&source, "content");
    env.assert_not_exists(&env.dotfiles_root.join("target/vimrc"));
}

#[test]
fn adopt_deploy_conflict_refused() {
    // Two packs, both would end up claiming ~/.vimrc after adoption.
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("vimrc", "existing")
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "new")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        "work",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "expected CrossPackConflict, got: {err}"
    );

    // Home untouched.
    env.assert_regular_file(&source, "new");
    // Pack copy rolled back.
    env.assert_not_exists(&env.dotfiles_root.join("work/vimrc"));
}

#[test]
fn adopt_deploy_conflict_not_bypassed_by_force() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("vimrc", "existing")
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "new")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        "work",
        std::slice::from_ref(&source),
        true, // --force should NOT bypass deploy conflicts
        false,
        false,
        &ctx,
    )
    .unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "--force must not bypass deploy conflicts, got: {err}"
    );
}

#[test]
fn adopt_dry_run_makes_no_changes() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file(".vimrc", "content")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    let result = commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        false,
        false,
        true, // dry-run
        &ctx,
    )
    .unwrap();
    assert!(result.dry_run);

    // Nothing changed at home.
    env.assert_regular_file(&source, "content");
    assert!(!env.fs.is_symlink(&source));
    // No copy in pack.
    env.assert_not_exists(&env.dotfiles_root.join("vim/vimrc"));
}

#[test]
fn adopt_no_follow_keeps_source_symlink_as_symlink() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .home_file("real_vimrc", "real content")
        .build();

    // ~/.vimrc is a symlink to ~/real_vimrc
    let real = env.home.join("real_vimrc");
    let source = env.home.join(".vimrc");
    env.fs.symlink(&real, &source).unwrap();

    let ctx = make_ctx(&env);
    commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        false,
        true, // --no-follow
        false,
        &ctx,
    )
    .unwrap();

    // The pack copy should be a symlink (not a regular file with copied content).
    let pack_copy = env.dotfiles_root.join("vim/vimrc");
    assert!(
        env.fs.is_symlink(&pack_copy),
        "--no-follow should preserve source symlink as a symlink in the pack"
    );
    // Home path replaced with a symlink into the pack.
    assert!(env.fs.is_symlink(&source));
}

#[cfg(unix)]
#[test]
fn adopt_force_preserves_old_content_when_copy_fails() {
    // With --force, the old destination must remain intact if the copy of
    // the new source fails. Previously copy_all removed the dest before
    // copying, so a copy failure silently lost the old content.
    use std::os::unix::fs::PermissionsExt;

    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "OLD")
        .done()
        .home_file(".vimrc", "NEW")
        .build();

    let source = env.home.join(".vimrc");
    // chmod 000 makes the file unreadable, so the copy phase fails at
    // read-time without tripping preflight (which uses lstat only).
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o000)).unwrap();

    let ctx = make_ctx(&env);
    let result = commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        true, // --force
        false,
        false,
        &ctx,
    );

    // Restore perms so drop-cleanup works regardless of assertion outcome.
    let _ = std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644));

    assert!(
        result.is_err(),
        "adopt should fail when the source is unreadable"
    );
    // The old pack content must survive the failed --force adoption.
    env.assert_regular_file(&env.dotfiles_root.join("vim/vimrc"), "OLD");
    // Home file also untouched.
    env.assert_regular_file(&source, "NEW");
    // No lingering stage file in the pack.
    let leftover = env.fs.read_dir(&env.dotfiles_root.join("vim")).unwrap();
    for entry in leftover {
        assert!(
            !entry.name.contains("dodot-adopt-stage"),
            "stage file leaked into pack: {}",
            entry.name
        );
    }
}

#[test]
fn adopt_no_follow_on_dangling_symlink_succeeds() {
    // A dangling symlink under --no-follow: readability check must inspect
    // the link itself (lstat), not try to follow it into a non-existent
    // target. Regression test: check_readable previously used fs.is_dir +
    // fs.stat, both of which follow symlinks and would fail here.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .build();

    // Create ~/.dangling -> /does/not/exist (target intentionally missing).
    let source = env.home.join(".dangling");
    env.fs
        .symlink(std::path::Path::new("/does/not/exist"), &source)
        .unwrap();

    let ctx = make_ctx(&env);
    commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        false,
        true, // --no-follow
        false,
        &ctx,
    )
    .expect("adopt with --no-follow on a dangling symlink should succeed");

    // The pack copy should itself be a symlink (preserving the dangling link).
    let pack_copy = env.dotfiles_root.join("vim/dangling");
    assert!(env.fs.is_symlink(&pack_copy));
    let target = env.fs.readlink(&pack_copy).unwrap();
    assert_eq!(target, std::path::PathBuf::from("/does/not/exist"));
}

#[test]
fn adopt_nonexistent_source_errors() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".does-not-exist");
    let err = commands::adopt::adopt(
        "vim",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();
    assert!(matches!(err, crate::DodotError::Fs { .. }), "got: {err}");
}

#[test]
fn adopt_empty_sources_errors() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("placeholder", "")
        .done()
        .build();
    let ctx = make_ctx(&env);
    let err = commands::adopt::adopt("vim", &[], false, false, false, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("no files"), "got: {msg}");
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
    let err = commands::adopt::adopt(
        "newpack",
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();
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

    // 5. Status after down — pending again. Dangling user-side symlinks
    // left by `down` are not conflicts (executor handles them on
    // re-deploy), so they stay plain pending.
    let s3 = commands::status::status(None, &ctx).unwrap();
    for pack in &s3.packs {
        for file in &pack.files {
            assert_eq!(
                file.status, "pending",
                "{} should be pending after down, got {}",
                file.name, file.status
            );
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

/// Regression for #43: status must distinguish "pending — clear to
/// deploy" from "pending — would conflict with a pre-existing file".
/// Both render under the `pending` *label*, but the conflict case gets
/// a `warning` status (so themes can color it differently) plus a
/// footnote explaining what's at the target path.
#[test]
fn status_surfaces_pre_existing_conflict_as_warning_with_footnote() {
    let env = TempEnvironment::builder()
        .pack("ghostty")
        .file("ghostrc", "theme=dark")
        .done()
        .home_file(".ghostrc", "theme=light")
        .pack("vim")
        .file("vimrc", "set nocompat")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    let ghostty = result
        .packs
        .iter()
        .find(|p| p.name == "ghostty")
        .expect("ghostty pack should appear");
    let vim = result
        .packs
        .iter()
        .find(|p| p.name == "vim")
        .expect("vim pack should appear");

    let ghostty_file = &ghostty.files[0];
    assert_eq!(
        ghostty_file.status, "warning",
        "ghostty/ghostrc collides with ~/.ghostrc — should surface as warning"
    );
    assert!(
        ghostty_file.status_label.starts_with("pending ("),
        "label should keep 'pending' and add a footnote ref, got: {}",
        ghostty_file.status_label
    );
    assert_eq!(
        ghostty.footnotes.len(),
        1,
        "ghostty should have exactly one footnote, got: {:?}",
        ghostty.footnotes
    );
    assert!(
        ghostty.footnotes[0].contains(".ghostrc"),
        "footnote should mention the conflicting path, got: {}",
        ghostty.footnotes[0]
    );
    assert!(
        ghostty.footnotes[0].contains("existing file"),
        "footnote should classify the target (existing file), got: {}",
        ghostty.footnotes[0]
    );

    // vim has no pre-existing ~/.vimrc — should be plain pending, no footnote.
    let vim_file = &vim.files[0];
    assert_eq!(
        vim_file.status, "pending",
        "vim/vimrc has no conflict — should be plain pending"
    );
    assert_eq!(vim_file.status_label, "pending");
    assert!(
        vim.footnotes.is_empty(),
        "vim should have no footnotes, got: {:?}",
        vim.footnotes
    );
}

/// Negative regression for #43: pre-existing symlinks at the user-target
/// path are NOT conflicts. The executor's `create_user_link` gracefully
/// replaces them (correct ones are no-ops, wrong/dangling ones are
/// removed and recreated), so flagging them would be a false positive.
/// Issue #44 may add an informational note for the equivalent-symlink
/// case, but that's separate from the conflict detection #43 introduces.
#[test]
fn status_does_not_flag_pre_existing_symlinks_as_conflict() {
    let env = TempEnvironment::builder()
        .pack("kitty")
        .file("kittyrc", "font_size 14")
        .done()
        .pack("ghostty")
        .file("ghostrc", "x")
        .done()
        .build();

    // Equivalent symlink: ~/.kittyrc already points at dodot's source.
    let source = env.dotfiles_root.join("kitty/kittyrc");
    let kitty_target = env.home.join(".kittyrc");
    env.fs.symlink(&source, &kitty_target).unwrap();

    // Non-equivalent symlink: ~/.ghostrc points somewhere else entirely.
    let ghostty_target = env.home.join(".ghostrc");
    env.fs
        .symlink(std::path::Path::new("/tmp/elsewhere"), &ghostty_target)
        .unwrap();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    let kitty = result.packs.iter().find(|p| p.name == "kitty").unwrap();
    assert_eq!(
        kitty.files[0].status, "pending",
        "equivalent symlink should be plain pending, not a conflict (executor handles it)"
    );
    assert!(
        kitty.footnotes.is_empty(),
        "no footnote for non-conflict, got: {:?}",
        kitty.footnotes
    );

    let ghostty = result.packs.iter().find(|p| p.name == "ghostty").unwrap();
    assert_eq!(
        ghostty.files[0].status, "pending",
        "non-equivalent symlink should also be plain pending — executor will replace it"
    );
    assert!(
        ghostty.footnotes.is_empty(),
        "no footnote for non-conflict, got: {:?}",
        ghostty.footnotes
    );
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
