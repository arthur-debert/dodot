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

/// CommandRunner test double that returns canned outputs per `(exe,
/// args...)` key. Used by probe::app integration tests so the brew /
/// mdls / mdfind subprocesses don't actually run.
struct CannedRunner {
    responses: std::sync::Mutex<std::collections::HashMap<Vec<String>, CommandOutput>>,
}

impl CannedRunner {
    fn new() -> Self {
        Self {
            responses: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
    fn respond(&self, args: &[&str], stdout: &str, exit_code: i32) {
        let key: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.responses.lock().unwrap().insert(
            key,
            CommandOutput {
                exit_code,
                stdout: stdout.into(),
                stderr: String::new(),
            },
        );
    }
}

impl CommandRunner for CannedRunner {
    fn run(&self, exe: &str, args: &[String]) -> Result<CommandOutput> {
        let mut full = vec![exe.to_string()];
        full.extend(args.iter().cloned());
        self.responses
            .lock()
            .unwrap()
            .get(&full)
            .cloned()
            .ok_or_else(|| {
                crate::DodotError::Other(format!("CannedRunner: no canned response for {full:?}"))
            })
    }
}

fn make_ctx(env: &TempEnvironment) -> ExecutionContext {
    let runner: Arc<dyn CommandRunner> = Arc::new(MockCommandRunner);
    let datastore = Arc::new(FilesystemDataStore::new(
        env.fs.clone(),
        env.paths.clone(),
        runner.clone(),
    ));
    let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());

    ExecutionContext {
        fs: env.fs.clone() as Arc<dyn Fs>,
        datastore,
        paths: env.paths.clone() as Arc<dyn Pather>,
        config_manager,
        syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
        command_runner: runner,
        dry_run: false,
        no_provision: true,
        provision_rerun: false,
        force: false,
        view_mode: crate::commands::ViewMode::Full,
        group_mode: crate::commands::GroupMode::Name,
        verbose: false,
    }
}

/// Variant of make_ctx that swaps in a [`CannedRunner`] so probe
/// tests can exercise the brew/mdls/mdfind enrichment paths without
/// spawning processes.
fn make_ctx_with_runner(env: &TempEnvironment, runner: Arc<dyn CommandRunner>) -> ExecutionContext {
    let datastore = Arc::new(FilesystemDataStore::new(
        env.fs.clone(),
        env.paths.clone(),
        runner.clone(),
    ));
    let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());
    ExecutionContext {
        fs: env.fs.clone() as Arc<dyn Fs>,
        datastore,
        paths: env.paths.clone() as Arc<dyn Pather>,
        config_manager,
        syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
        command_runner: runner,
        dry_run: false,
        no_provision: true,
        provision_rerun: false,
        force: false,
        view_mode: crate::commands::ViewMode::Full,
        group_mode: crate::commands::GroupMode::Name,
        verbose: false,
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

/// On non-macOS, `_lib/<rest>` entries resolve to `Resolution::Skip`
/// in the planner. Status must suppress the corresponding row and
/// only surface the warning — otherwise the user sees a confusing
/// "pending symlink" row alongside a "skipping on this platform"
/// warning. Regression for review feedback on PR #90.
#[test]
fn status_suppresses_lib_prefix_rows_when_skipped() {
    let env = TempEnvironment::builder()
        .pack("macapps")
        .file("_lib/LaunchAgents/com.example.foo.plist", "x")
        .file("regular.toml", "y")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    // The pack is present either way; what we're pinning is the
    // *file rows*: on non-macOS the `_lib/...` row is suppressed,
    // on macOS it appears like any other pending symlink.
    let pack = result
        .packs
        .iter()
        .find(|p| p.name == "macapps")
        .expect("macapps pack must appear");

    let lib_row = pack
        .files
        .iter()
        .find(|f| f.name.starts_with("_lib/") || f.name == "_lib");
    let regular_row = pack.files.iter().find(|f| f.name == "regular.toml");

    assert!(
        regular_row.is_some(),
        "non-_lib entry must always render; got files {:?}",
        pack.files.iter().map(|f| &f.name).collect::<Vec<_>>()
    );

    if cfg!(target_os = "macos") {
        assert!(
            lib_row.is_some(),
            "on macOS `_lib/` entries should render normally; got files {:?}",
            pack.files.iter().map(|f| &f.name).collect::<Vec<_>>()
        );
    } else {
        assert!(
            lib_row.is_none(),
            "on non-macOS `_lib/` rows must be suppressed; got files {:?}",
            pack.files.iter().map(|f| &f.name).collect::<Vec<_>>()
        );
        // The warning channel still carries the explanation. The
        // exact form depends on whether the catchall scanner matched
        // the top-level `_lib` directory or a nested `_lib/<rest>`
        // file — either way, the warning mentions `_lib` and the
        // macOS-only constraint.
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("_lib") && w.contains("macOS-only")),
            "expected a `_lib` macOS-only warning; got {:?}",
            result.warnings
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
fn status_surfaces_syntax_error_sidecar_for_deployed_shell_file() {
    use crate::shell::{SyntaxCheckResult, SyntaxChecker};
    use std::path::Path;

    struct FlagAliases;
    impl SyntaxChecker for FlagAliases {
        fn check(&self, _interpreter: &str, file: &Path) -> SyntaxCheckResult {
            if file.file_name().and_then(|s| s.to_str()) == Some("aliases.sh") {
                SyntaxCheckResult::SyntaxError {
                    stderr: "/path/aliases.sh: line 47: bad substitution\n".into(),
                }
            } else {
                SyntaxCheckResult::Ok
            }
        }
    }

    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "echo ${broken")
        .file("env.sh", "export FOO=bar")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.syntax_checker = Arc::new(FlagAliases);
    commands::up::up(None, &ctx).unwrap();

    // Now run status — should flag aliases.sh as broken and leave
    // env.sh as plain deployed.
    let result = commands::status::status(None, &ctx).unwrap();
    let pack = &result.packs[0];

    let aliases = pack
        .files
        .iter()
        .find(|f| f.name == "aliases.sh")
        .expect("aliases.sh row missing");
    assert_eq!(aliases.status, "broken", "row: {aliases:?}");
    assert_eq!(aliases.status_label, "syntax error");
    let note_idx = aliases
        .note_ref
        .expect("aliases.sh should carry a note ref") as usize;
    assert!(
        result.notes[note_idx - 1].body.contains("bad substitution"),
        "note: {:?}",
        result.notes[note_idx - 1]
    );

    let env_row = pack
        .files
        .iter()
        .find(|f| f.name == "env.sh")
        .expect("env.sh row missing");
    assert_eq!(env_row.status, "deployed");
    assert_eq!(env_row.status_label, "sourced");
}

#[test]
fn status_surfaces_runtime_failures_from_recent_profiles() {
    // A clean source (passes syntax) that exits non-zero in 2 of the
    // 3 most recent shell startups should be flagged as broken with a
    // footnote summarizing the failure rate.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    // Write three fake profile TSVs by hand. Distinct exit codes for
    // the old vs. new failure — if the aggregator overwrites
    // last_failure_exit while iterating newest→oldest, this test
    // catches it: oldest=2, newest=1, so the label must say "exited 1"
    // (the most-recent failure), not "exited 2".
    let source_path = env.dotfiles_root.join("vim/aliases.sh");
    let target = source_path.display().to_string();
    let probes_dir = env.paths.probes_shell_init_dir();
    env.fs.mkdir_all(&probes_dir).unwrap();
    let make_profile = |t0: u64, exit: i32| {
        let body = format!(
            "# dodot shell-init profile v1\n\
             # shell\tbash 5.0\n\
             # start_t\t{t0}.000000\n\
             source\tvim\tshell\t{target}\t{t0}.000100\t{t0}.000900\t{exit}\n\
             # end_t\t{t0}.001000\n",
        );
        env.fs
            .write_file(
                &probes_dir.join(format!("profile-{t0:010}-100-1.tsv")),
                body.as_bytes(),
            )
            .unwrap();
    };
    make_profile(1714000001, 2); // oldest failure
    make_profile(1714000002, 0); // clean run in the middle
    make_profile(1714000003, 1); // most recent failure

    let result = commands::status::status(None, &ctx).unwrap();
    let row = result.packs[0]
        .files
        .iter()
        .find(|f| f.name == "aliases.sh")
        .expect("aliases.sh row missing");

    assert_eq!(row.status, "broken", "row: {row:?}");
    // Label must report the *most recent* failure's exit code (1),
    // not the older one (2).
    assert!(
        row.status_label.contains("exited 1") && row.status_label.contains("2/3"),
        "status_label was: {}",
        row.status_label
    );
    assert!(
        !row.status_label.contains("exited 2"),
        "status_label should report newest failure, not older: {}",
        row.status_label
    );
    let note_idx = row.note_ref.expect("expected note ref") as usize;
    let note = &result.notes[note_idx - 1];
    assert!(
        note.body.contains("2 of 3 recent shell startups"),
        "note body: {}",
        note.body
    );
    assert!(
        note.body.contains("last failure: exit 1"),
        "note should mention most recent failure exit code: {}",
        note.body
    );
    // The footnote should point users at the per-file probe view (not
    // `--history`, which only shows aggregate counts).
    assert!(
        note.body.contains("dodot probe shell-init vim/aliases.sh"),
        "note should point at the filtered probe view: {}",
        note.body
    );
}

#[test]
fn status_inlines_captured_stderr_into_runtime_failure_footnote() {
    // When a recent failing run also has a sibling errors.log entry,
    // the status footnote should inline a snippet of the stderr so the
    // user sees the actual error inline, not just the exit code.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build();
    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let source_path = env.dotfiles_root.join("vim/aliases.sh");
    let target = source_path.display().to_string();
    let probes_dir = env.paths.probes_shell_init_dir();
    env.fs.mkdir_all(&probes_dir).unwrap();
    let prof_name = "profile-1714000003-100-1.tsv";
    let body = format!(
        "# dodot shell-init profile v1\n\
         # shell\tbash 5.0\n\
         # start_t\t1714000003.000000\n\
         source\tvim\tshell\t{target}\t1714000003.000100\t1714000003.000900\t1\n\
         # end_t\t1714000003.001000\n",
    );
    env.fs
        .write_file(&probes_dir.join(prof_name), body.as_bytes())
        .unwrap();
    let err_log = format!(
        "# dodot shell-init errors v1\n@@\t{target}\t1\nzsh: command not found: gpg-agent\n"
    );
    env.fs
        .write_file(
            &probes_dir.join("profile-1714000003-100-1.errors.log"),
            err_log.as_bytes(),
        )
        .unwrap();

    let result = commands::status::status(None, &ctx).unwrap();
    let row = result.packs[0]
        .files
        .iter()
        .find(|f| f.name == "aliases.sh")
        .expect("aliases.sh row missing");
    let note_idx = row.note_ref.expect("expected note ref") as usize;
    let note = &result.notes[note_idx - 1];
    assert!(
        note.body.contains("stderr:"),
        "footnote should label the stderr excerpt: {}",
        note.body
    );
    assert!(
        note.body.contains("zsh: command not found: gpg-agent"),
        "footnote should inline the captured stderr: {}",
        note.body
    );
    assert!(
        note.body.contains("dodot probe shell-init vim/aliases.sh"),
        "footnote should point at the per-file probe view: {}",
        note.body
    );
}

#[test]
fn up_writes_syntax_error_sidecar_when_check_fails() {
    use crate::shell::{SyntaxCheckResult, SyntaxChecker};
    use std::path::Path;

    // A checker that flags `aliases.sh` as broken so we can verify
    // up wires the validation pass through correctly.
    struct FlagAliases;
    impl SyntaxChecker for FlagAliases {
        fn check(&self, _interpreter: &str, file: &Path) -> SyntaxCheckResult {
            if file.file_name().and_then(|s| s.to_str()) == Some("aliases.sh") {
                SyntaxCheckResult::SyntaxError {
                    stderr: "aliases.sh: line 1: unexpected token\n".into(),
                }
            } else {
                SyntaxCheckResult::Ok
            }
        }
    }

    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "if [ x = y\nfi")
        .file("env.sh", "export FOO=bar")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.syntax_checker = Arc::new(FlagAliases);
    commands::up::up(None, &ctx).unwrap();

    // Sidecar present for the failing file…
    let bad = crate::shell::error_sidecar_path(env.paths.as_ref(), "vim", "aliases.sh");
    assert!(env.fs.exists(&bad), "expected sidecar at {}", bad.display());
    let body = env.fs.read_to_string(&bad).unwrap();
    assert!(body.contains("unexpected token"), "sidecar:\n{body}");

    // …and not for the clean file.
    let good = crate::shell::error_sidecar_path(env.paths.as_ref(), "vim", "env.sh");
    assert!(!env.fs.exists(&good));
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

#[test]
fn up_dry_run_does_not_write_preprocessing_baselines() {
    // Baselines anchor "the state of the last successful `up`," so
    // a dry run — which never executes — must not move that anchor.
    let env = TempEnvironment::builder()
        .pack("app")
        .file("config.toml.tmpl", "name = {{ name }}")
        .config("[preprocessor.template.vars]\nname = \"Alice\"\n")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let baseline_path = ctx
        .paths
        .preprocessor_baseline_path("app", "preprocessed", "config.toml");
    assert!(
        !ctx.fs.exists(&baseline_path),
        "test precondition: baseline should not exist before any up runs"
    );

    let mut dry_ctx = make_ctx(&env);
    dry_ctx.dry_run = true;
    let _ = commands::up::up(None, &dry_ctx).unwrap();

    assert!(
        !ctx.fs.exists(&baseline_path),
        "dry-run must NOT write a baseline; the cache must remain untouched"
    );
}

// ── up: conflict handling ──────────────────────────────────

#[test]
fn up_reports_conflict_when_file_exists() {
    // home.gitconfig (post-#48 per-file home opt-in) routes to ~/.gitconfig,
    // which already exists in the home_file fixture. That collision
    // exercises the conflict path the test cares about.
    let env = TempEnvironment::builder()
        .pack("git")
        .file("home.gitconfig", "[user]\n  name = new")
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
    // The conflict message now lives in the notes section, referenced by
    // the error row's note_ref. status_label stays a short "error" keyword
    // so the column layout is preserved.
    let note_idx = error_files[0]
        .note_ref
        .expect("error row should carry a note_ref") as usize
        - 1;
    assert!(
        result.notes[note_idx].body.contains("conflict"),
        "note should mention conflict: {}",
        result.notes[note_idx].body
    );
    // Error rows must identify the failing file in the left column,
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
    assert_eq!(
        conflicted.status_label, "pending",
        "warning label should be plain 'pending' (the [N] marker is a separate column now), got: {}",
        conflicted.status_label
    );
    assert!(
        conflicted.note_ref.is_some(),
        "conflicted row should carry a note_ref into the command-wide notes list"
    );
    assert!(
        !status.notes.is_empty(),
        "status should have at least one note describing the pre-existing file"
    );
    let note_idx = conflicted.note_ref.unwrap() as usize - 1;
    assert!(
        status.notes[note_idx].body.contains(".gitconfig"),
        "note should mention the conflicting path, got: {}",
        status.notes[note_idx].body
    );
}

#[test]
fn up_force_overwrites_existing_files() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("home.gitconfig", "[user]\n  name = new")
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

// ── up: reconcile non-provisioning state (#58) ─────────────

/// `dodot up` was additive only: a deleted source file would leave its
/// datastore entry behind, so the regenerated init script kept sourcing
/// a now-missing path. The fix wipes configuration-handler state per
/// pack at the start of `up` and re-applies from current source.
#[test]
fn up_reconciles_deleted_shell_source() {
    let env = TempEnvironment::builder()
        .pack("gh")
        .file("aliases.sh", "alias g=git")
        .file("profile.sh", "export GH=true")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let shell_dir = env.paths.handler_data_dir("gh", "shell");
    let mut before = env.list_dir_names(&shell_dir);
    before.sort();
    assert_eq!(before, vec!["aliases.sh", "profile.sh"]);

    // Delete one source from the pack and re-run up.
    env.fs
        .remove_file(&env.dotfiles_root.join("gh/profile.sh"))
        .unwrap();
    commands::up::up(None, &ctx).unwrap();

    let after = env.list_dir_names(&shell_dir);
    assert_eq!(
        after,
        vec!["aliases.sh"],
        "orphan datastore entry persisted after re-up"
    );

    let init = env
        .fs
        .read_to_string(&env.paths.init_script_path())
        .unwrap();
    assert!(
        !init.contains("profile.sh"),
        "regenerated init still references deleted file:\n{init}"
    );
    assert!(init.contains("aliases.sh"), "init: {init}");
}

#[test]
fn up_reconciles_deleted_symlink_source() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .file("gvimrc", "set guifont=Mono")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let symlink_dir = env.paths.handler_data_dir("vim", "symlink");
    let mut before = env.list_dir_names(&symlink_dir);
    before.sort();
    assert_eq!(before, vec!["gvimrc", "vimrc"]);

    env.fs
        .remove_file(&env.dotfiles_root.join("vim/gvimrc"))
        .unwrap();
    commands::up::up(None, &ctx).unwrap();

    let after = env.list_dir_names(&symlink_dir);
    assert_eq!(
        after,
        vec!["vimrc"],
        "orphan datastore symlink persisted after re-up"
    );
}

#[test]
fn up_reconciles_deleted_path_dir() {
    let env = TempEnvironment::builder()
        .pack("tools")
        .file("bin/foo", "#!/bin/sh\necho foo")
        .file("vimrc", "set nocompatible")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let path_dir = env.paths.handler_data_dir("tools", "path");
    assert_eq!(env.list_dir_names(&path_dir), vec!["bin"]);

    // Drop the bin/ directory entirely.
    env.fs
        .remove_dir_all(&env.dotfiles_root.join("tools/bin"))
        .unwrap();
    commands::up::up(None, &ctx).unwrap();

    let after = env.list_dir_names(&path_dir);
    assert!(
        after.is_empty(),
        "path datastore should be empty after source dir removed, got: {after:?}"
    );

    let init = env
        .fs
        .read_to_string(&env.paths.init_script_path())
        .unwrap();
    assert!(
        !init.contains("tools/bin"),
        "init script still exports deleted PATH entry:\n{init}"
    );
}

/// Provisioning handlers (install, homebrew) must NOT be wiped — their
/// sentinels record "did this run with this content?" and re-running
/// would defeat the point of sentinels (reinstall on every up).
#[test]
fn up_preserves_install_sentinel_when_source_persists() {
    let env = TempEnvironment::builder()
        .pack("setup")
        .file("install.sh", "#!/bin/sh\necho hi")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;

    commands::up::up(None, &ctx).unwrap();

    let install_dir = env.paths.handler_data_dir("setup", "install");
    let sentinels_before = env.list_dir_names(&install_dir);
    assert_eq!(
        sentinels_before.len(),
        1,
        "expected one sentinel, got {sentinels_before:?}"
    );
    let original = sentinels_before.into_iter().next().unwrap();

    // Re-run up with no source change. The sentinel must persist —
    // wiping it would force the script to re-execute every time.
    commands::up::up(None, &ctx).unwrap();
    let sentinels_after = env.list_dir_names(&install_dir);
    assert_eq!(
        sentinels_after,
        vec![original],
        "install sentinel should persist across re-up"
    );
}

#[test]
fn up_preserves_install_sentinel_when_source_deleted() {
    let env = TempEnvironment::builder()
        .pack("setup")
        .file("install.sh", "#!/bin/sh\necho hi")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;

    commands::up::up(None, &ctx).unwrap();
    let install_dir = env.paths.handler_data_dir("setup", "install");
    let sentinels_before = env.list_dir_names(&install_dir);
    assert_eq!(sentinels_before.len(), 1);

    // Source vanishes — but the sentinel still records that *some*
    // version of this script has run, and we don't want the wipe to
    // erase that history just because the source is no longer in the
    // pack right now.
    env.fs
        .remove_file(&env.dotfiles_root.join("setup/install.sh"))
        .unwrap();
    commands::up::up(None, &ctx).unwrap();

    let sentinels_after = env.list_dir_names(&install_dir);
    assert_eq!(
        sentinels_after, sentinels_before,
        "deleting an install source must not wipe its sentinel"
    );
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
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // File should have moved into pack with the `home.` prefix (post-#48
    // adopt rename — preserves the round-trip back to ~/.vimrc on `up`),
    // content preserved.
    env.assert_regular_file(
        &env.dotfiles_root.join("vim/home.vimrc"),
        "set nocompatible",
    );
    // Symlink should exist at original location
    assert!(env.fs.is_symlink(&source));

    // Status output should include the vim pack with the adopted file
    assert!(result.packs.iter().any(|p| p.name == "vim"));
    let vim = result.packs.iter().find(|p| p.name == "vim").unwrap();
    assert!(vim.files.iter().any(|f| f.name == "home.vimrc"));
}

#[test]
fn adopt_preserves_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;

    // Uses a dotted file (post-#2: non-dotted $HOME entries are
    // refused for round-trip safety). The test's intent is exec-bit
    // preservation, not the dot-or-not policy.
    let env = TempEnvironment::builder()
        .pack("tools")
        .file("placeholder", "")
        .done()
        .home_file(".script.sh", "#!/bin/sh\necho hi")
        .build();

    let source = env.home.join(".script.sh");
    // Mark source as executable
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&source, perms).unwrap();

    let ctx = make_ctx(&env);
    commands::adopt::adopt(
        Some("tools"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    let dest = env.dotfiles_root.join("tools/home.script.sh");
    let meta = std::fs::metadata(&dest).unwrap();
    assert_eq!(
        meta.permissions().mode() & 0o777,
        0o755,
        "executable bit should be preserved on adopted file"
    );
}

/// Regression for review item #2 on PR #49: a non-dotted entry in
/// $HOME has no automatic round-trip path under the post-#48 XDG
/// default — adopt must refuse rather than silently relocate.
#[test]
fn adopt_refuses_non_dotted_home_entry() {
    let env = TempEnvironment::builder()
        .pack("tools")
        .file("placeholder", "")
        .done()
        .home_file("script.sh", "#!/bin/sh\necho hi")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join("script.sh");
    let err = commands::adopt::adopt(
        Some("tools"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("non-dotted entry in $HOME"),
        "expected refusal message, got: {msg}"
    );
    assert!(
        msg.contains("[symlink.targets]"),
        "refusal should point at [symlink.targets] escape hatch, got: {msg}"
    );
    // Source untouched, no pack copy created.
    env.assert_regular_file(&source, "#!/bin/sh\necho hi");
    env.assert_not_exists(&env.dotfiles_root.join("tools/script.sh"));
}

#[test]
fn adopt_destination_conflict_refused_without_force() {
    // Destination conflict: pack already has `home.vimrc`. Adopt of
    // `~/.vimrc` derives `home.vimrc` as the pack filename (post-#48
    // adopt rename), so the existing file blocks the adoption.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("home.vimrc", "existing content")
        .done()
        .home_file(".vimrc", "new content")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    let err = commands::adopt::adopt(
        Some("vim"),
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
    env.assert_regular_file(
        &env.dotfiles_root.join("vim/home.vimrc"),
        "existing content",
    );
}

#[test]
fn adopt_destination_conflict_resolved_with_force() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("home.vimrc", "OLD")
        .done()
        .home_file(".vimrc", "NEW")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");

    commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        true, // --force
        false,
        false,
        &ctx,
    )
    .unwrap();

    env.assert_regular_file(&env.dotfiles_root.join("vim/home.vimrc"), "NEW");
    assert!(env.fs.is_symlink(&source));
}

#[test]
fn adopt_directory_creates_symlink_and_preserves_contents() {
    // Dotted-directory adoption from $HOME directly: contents move to
    // pack/_home/<stripped>/, which round-trips back via the `_home/`
    // subtree-escape (Priority 2) on `dodot up`. We use a non-XDG
    // dotted dir so the test stays decoupled from the XDG-source
    // inference rules — adopting `~/.config/` itself is now refused
    // explicitly (see `adopt_xdg_root_itself_refused`).
    let env = TempEnvironment::builder()
        .pack("editor")
        .file("placeholder", "")
        .done()
        .home_file(".vim/vimrc", "set nocompatible")
        .home_file(".vim/colors/scheme.vim", "\" colors")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vim");

    commands::adopt::adopt(
        Some("editor"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    let pack_dir = env.dotfiles_root.join("editor/_home/vim");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("vimrc"), "set nocompatible");
    env.assert_regular_file(&pack_dir.join("colors/scheme.vim"), "\" colors");

    // Original path is now a symlink to the pack copy.
    assert!(env.fs.is_symlink(&source));
    let target = env.fs.readlink(&source).unwrap();
    assert_eq!(target, pack_dir);
}

/// Regression for review item #1 on PR #49: a dotted directory adopted
/// from $HOME (not in force_home) must round-trip back via the
/// `_home/` escape hatch on `dodot up`. Without this, the file would
/// silently move from $HOME/.X to $XDG_CONFIG_HOME/<pack>/X.
#[test]
fn adopt_dotted_dir_from_home_round_trips_via_home_escape() {
    let env = TempEnvironment::builder()
        .pack("chats")
        .file("placeholder", "")
        .done()
        .home_file(".weechat/weechat.conf", "[server]")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".weechat");

    commands::adopt::adopt(
        Some("chats"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // Adopted under chats/_home/weechat (the `_home/` per-subtree
    // routing tells the symlink handler to deploy back to $HOME/.X).
    let pack_dir = env.dotfiles_root.join("chats/_home/weechat");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("weechat.conf"), "[server]");

    // Re-deploying with `dodot up` puts the symlink back at $HOME/.weechat
    // — the round-trip the rename was designed to preserve.
    commands::up::up(Some(&["chats".into()]), &ctx).unwrap();
    let user_path = env.home.join(".weechat");
    assert!(
        env.fs.is_symlink(&user_path),
        "~/.weechat should be a symlink after re-deploy"
    );
}

/// **Round-trip property** — the critical contract between `adopt` and
/// `resolve_target`. For every `$HOME` source that `adopt` accepts,
/// feeding the `derive_pack_filename` result back through
/// `resolve_target` must return the original source path.
///
/// `derive_pack_filename` encodes the *inverse* of `resolve_target`'s
/// priority rules (force_home, home. prefix, _home/ directory). The
/// two functions are separately implemented but must stay lockstep;
/// this test catches any drift directly.
///
/// Cases cover every accepted branch:
///   - force_home file (`~/.bashrc`)
///   - force_home directory (`~/.ssh`)
///   - dotted non-force_home file (`~/.vimrc`)
///   - dotted non-force_home directory (`~/.weechat`)
///
/// The refused branch (non-dotted $HOME entry) is covered by the
/// explicit refusal test `adopt_refuses_non_dotted_home_entry`.
#[test]
fn pack_filename_round_trips_through_resolve_target() {
    use crate::commands::adopt::derive_pack_filename;
    use crate::handlers::symlink::resolve_target;

    // Default force_home: match what dodot ships (keep this minimal
    // and explicit so test failures point at a real behavior change).
    let force_home: Vec<String> = vec![
        "ssh".into(),
        "gnupg".into(),
        "aws".into(),
        "kube".into(),
        "bashrc".into(),
        "zshrc".into(),
        "profile".into(),
        "inputrc".into(),
    ];
    let config = crate::handlers::HandlerConfig {
        force_home: force_home.clone(),
        ..crate::handlers::HandlerConfig::default()
    };

    let paths = crate::paths::XdgPather::builder()
        .home("/home/alice")
        .dotfiles_root("/home/alice/dotfiles")
        .xdg_config_home("/home/alice/.config")
        .build()
        .unwrap();

    struct Case {
        pack: &'static str,
        // The file/dir name as it would appear inside $HOME (.vimrc, .ssh, …).
        home_name: &'static str,
        is_dir: bool,
        // What `derive_pack_filename` should produce (here as documentation; the
        // test only asserts the round-trip, not the literal pack filename — a
        // future refactor of the inverse rules is allowed to pick a different
        // internal representation as long as the round-trip still holds).
        expected_pack_filename: &'static str,
    }

    let cases = [
        Case {
            pack: "shell",
            home_name: ".bashrc",
            is_dir: false,
            expected_pack_filename: "bashrc",
        },
        Case {
            pack: "net",
            home_name: ".ssh",
            is_dir: true,
            expected_pack_filename: "ssh",
        },
        Case {
            pack: "vim",
            home_name: ".vimrc",
            is_dir: false,
            expected_pack_filename: "home.vimrc",
        },
        Case {
            pack: "chats",
            home_name: ".weechat",
            is_dir: true,
            expected_pack_filename: "_home/weechat",
        },
    ];

    for c in &cases {
        let derived =
            derive_pack_filename(c.home_name, c.is_dir, &force_home).unwrap_or_else(|e| {
                panic!(
                    "derive_pack_filename refused accepted case {:?}: {e}",
                    c.home_name
                )
            });
        assert_eq!(
            derived, c.expected_pack_filename,
            "documentation-expected pack filename drifted for {}",
            c.home_name
        );

        let target = resolve_target(c.pack, &derived, &config, &paths);
        let expected_source = std::path::PathBuf::from(format!("/home/alice/{}", c.home_name));
        assert_eq!(
            target,
            expected_source,
            "round-trip broke for {}: derive_pack_filename → {} → resolve_target → {} \
             (expected back at {})",
            c.home_name,
            derived,
            target.display(),
            expected_source.display(),
        );
    }

    // Refused case: non-dotted entry — no round-trip path exists.
    let refused = derive_pack_filename("my_script.sh", false, &force_home);
    assert!(
        refused.is_err(),
        "non-dotted $HOME entry must be refused, got: {refused:?}"
    );
}

#[test]
fn adopt_preserves_inner_symlinks_as_symlinks() {
    // Uses a dotted directory (post-#2: non-dotted $HOME entries are
    // refused). Test intent: inner symlinks are preserved during the
    // copy phase. The `_home/` path comes from #1's dotted-dir
    // round-trip rename.
    let env = TempEnvironment::builder()
        .pack("shell")
        .file("placeholder", "")
        .done()
        .home_file(".mydir/real.txt", "hello")
        .build();

    // Create an inner symlink: .mydir/alias -> .mydir/real.txt
    let inner_target = env.home.join(".mydir/real.txt");
    let inner_link = env.home.join(".mydir/alias");
    env.fs.symlink(&inner_target, &inner_link).unwrap();

    let ctx = make_ctx(&env);
    let source = env.home.join(".mydir");
    commands::adopt::adopt(
        Some("shell"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // The inner link should still be a symlink inside the pack copy.
    let copied_link = env.dotfiles_root.join("shell/_home/mydir/alias");
    assert!(
        env.fs.is_symlink(&copied_link),
        "inner symlink should be preserved as a symlink, not followed"
    );
}

/// `~/.config/<X>/<rest>` is now a recognized adopt source: the first
/// segment under `$XDG_CONFIG_HOME` is the inferred pack name, and the
/// remainder is the in-pack path. Round-trip is the resolver's default
/// rule — pack `nvim` containing `init.lua` deploys to
/// `$XDG_CONFIG_HOME/nvim/init.lua` on `dodot up`. (Pre-inference, this
/// case was refused as a nested source.)
#[test]
fn adopt_xdg_nested_file_lands_at_pack_root() {
    let env = TempEnvironment::builder()
        .pack("nvim")
        .file("placeholder", "")
        .done()
        .home_file(".config/nvim/init.lua", "-- config")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/nvim/init.lua");

    commands::adopt::adopt(
        Some("nvim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // No prefix gymnastics: pack `nvim`'s default deploy rule (Priority
    // 4 — `$XDG/<pack>/<rel>`) lands `init.lua` back at the original
    // `~/.config/nvim/init.lua`. So the in-pack name is just `init.lua`.
    let pack_file = env.dotfiles_root.join("nvim/init.lua");
    env.assert_regular_file(&pack_file, "-- config");
    assert!(env.fs.is_symlink(&source));
    let target = env.fs.readlink(&source).unwrap();
    assert_eq!(target, pack_file);
}

/// Pack name can be omitted when the source carries pack structure
/// under `$XDG_CONFIG_HOME`: `dodot adopt ~/.config/nvim/init.lua` (no
/// `--into`) auto-detects pack `nvim` and creates it if missing.
#[test]
fn adopt_xdg_source_infers_pack_and_auto_creates() {
    let env = TempEnvironment::builder()
        .home_file(".config/ghostty/config", "theme = dark")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/ghostty/config");

    commands::adopt::adopt(
        /*pack_override=*/ None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // Pack auto-created at `<dotfiles>/ghostty/`, file landed at root.
    let pack_dir = env.dotfiles_root.join("ghostty");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("config"), "theme = dark");
    assert!(env.fs.is_symlink(&source));
}

/// Adopting `~/.config/<X>/` (the pack-root directory itself) expands
/// into per-child plans rather than making the directory one big
/// symlink-to-pack-root. Each top-level entry becomes a top-level pack
/// member, so `dodot up` deploys per-entry like any other pack.
#[test]
fn adopt_xdg_pack_root_directory_expands_to_children() {
    let env = TempEnvironment::builder()
        .home_file(".config/helix/config.toml", "theme = \"onedark\"")
        .home_file(".config/helix/themes/extra.toml", "fg = \"white\"")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/helix");

    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // Each top-level child of `~/.config/helix/` became its own pack
    // entry — `config.toml` (file) and `themes/` (dir) — both at pack
    // root, not nested under another `helix/`.
    let pack_dir = env.dotfiles_root.join("helix");
    env.assert_regular_file(&pack_dir.join("config.toml"), "theme = \"onedark\"");
    env.assert_regular_file(&pack_dir.join("themes/extra.toml"), "fg = \"white\"");
    // Original entries are now symlinks at their original paths
    // (one per top-level child, not one for the whole helix/ dir).
    assert!(env
        .fs
        .is_symlink(&env.home.join(".config/helix/config.toml")));
    assert!(env.fs.is_symlink(&env.home.join(".config/helix/themes")));
    // Parent directory `~/.config/helix/` itself stays a real directory
    // — only its children became symlinks.
    assert!(!env.fs.is_symlink(&source));
}

/// `~/.config/` itself is too broad to adopt as a single unit; refuse
/// explicitly so the user adopts an app subdirectory instead.
#[test]
fn adopt_xdg_root_itself_refused() {
    let env = TempEnvironment::builder()
        .home_file(".config/nvim/init.lua", "-- config")
        .build();
    let ctx = make_ctx(&env);
    let source = env.config_home.clone();

    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("$XDG_CONFIG_HOME"),
        "expected XDG-root refusal, got: {msg}"
    );
}

/// Pack-root directory expansion under `--into` reroute keeps the
/// `_xdg/<X>/` prefix on each child so the round-trip survives the
/// pack-name change. Without this, expanded children would land at
/// pack root and `dodot up` would deploy them to `$XDG/<override>/...`
/// instead of the original `$XDG/<X>/...`. (Regression for Copilot
/// review on PR #85.)
#[test]
fn adopt_xdg_pack_root_expansion_with_override_uses_xdg_prefix() {
    let env = TempEnvironment::builder()
        .pack("toolbox")
        .file("placeholder", "")
        .done()
        .home_file(".config/lazygit/config.yml", "gui:\n  theme: dark")
        .home_file(".config/lazygit/themes/x.yml", "fg: white")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/lazygit");

    commands::adopt::adopt(
        Some("toolbox"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // Each expanded child lives under `toolbox/_xdg/lazygit/...` so the
    // resolver's Priority 2 `_xdg/` prefix routes back to
    // `~/.config/lazygit/<child>` regardless of the override pack name.
    env.assert_regular_file(
        &env.dotfiles_root.join("toolbox/_xdg/lazygit/config.yml"),
        "gui:\n  theme: dark",
    );
    env.assert_regular_file(
        &env.dotfiles_root.join("toolbox/_xdg/lazygit/themes/x.yml"),
        "fg: white",
    );
    // Each original child is now a symlink (per-child expansion); the
    // pack-root dir itself stays a real directory.
    assert!(env
        .fs
        .is_symlink(&env.home.join(".config/lazygit/config.yml")));
    assert!(env.fs.is_symlink(&env.home.join(".config/lazygit/themes")));
    assert!(!env.fs.is_symlink(&source));
}

/// `--into <pack>` for an XDG source where the override differs from
/// the inferred pack name uses `_xdg/<X>/<rest>` so round-trip via
/// Priority 2 still lands the deployed file at the original location.
#[test]
fn adopt_xdg_with_into_override_uses_xdg_prefix() {
    let env = TempEnvironment::builder()
        .pack("toolbox")
        .file("placeholder", "")
        .done()
        .home_file(".config/lazygit/config.yml", "gui:\n  theme: dark")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/lazygit/config.yml");

    commands::adopt::adopt(
        Some("toolbox"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // Round-trip via `_xdg/lazygit/config.yml`: the `_xdg/` prefix
    // bypasses pack-namespacing so the deployed path is still
    // `~/.config/lazygit/config.yml` despite the file living in
    // pack `toolbox`.
    let pack_file = env.dotfiles_root.join("toolbox/_xdg/lazygit/config.yml");
    env.assert_regular_file(&pack_file, "gui:\n  theme: dark");
    assert!(env.fs.is_symlink(&source));
}

/// Adopting a file under `~/Library/Application Support/<X>/` infers
/// pack `<X>`, places the file at `_app/<X>/<rest>` in the pack tree,
/// and round-trips via the resolver's Priority 2c `_app/` prefix back
/// to the original AppSupport location. The `TempEnvironment` pins
/// `app_support_dir` under the temp HOME on every platform so this
/// test runs identically on Linux and macOS.
#[test]
fn adopt_app_support_source_round_trips_through_app_prefix() {
    let env = TempEnvironment::builder()
        .home_file(
            "Library/Application Support/Code/User/settings.json",
            "{\"editor.fontSize\": 14}",
        )
        .build();

    let ctx = make_ctx(&env);
    let source = env.app_support.join("Code/User/settings.json");

    commands::adopt::adopt(
        /*pack_override=*/ None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // Pack auto-created at `<dotfiles>/Code/`. The file lives at
    // `_app/Code/User/settings.json` — the prefix is mandatory at
    // natural pack name because the default rule routes through XDG,
    // not app_support_dir.
    let pack_file = env.dotfiles_root.join("Code/_app/Code/User/settings.json");
    env.assert_regular_file(&pack_file, "{\"editor.fontSize\": 14}");

    // Original deploy location is now a symlink — and the symlink
    // chain points (eventually) back at the pack copy. Resolve via
    // resolve_target_full to confirm round-trip.
    assert!(env.fs.is_symlink(&source));

    use crate::handlers::symlink::{resolve_target_full, Resolution};
    let resolution = resolve_target_full(
        "Code",
        "_app/Code/User/settings.json",
        &Default::default(),
        env.paths.as_ref(),
    );
    match resolution {
        Resolution::Path(p) => assert_eq!(p, source),
        Resolution::Skip { reason } => panic!("expected Path, got Skip({reason})"),
    }
}

/// Adopting `~/Library/Application Support/<X>/` (the directory
/// itself) expands into per-child plans, mirroring the XDG pack-root
/// expansion. Each top-level entry under the AppSupport folder
/// becomes a top-level pack entry, prefixed with `_app/<X>/`.
#[test]
fn adopt_app_support_pack_root_directory_expands_to_children() {
    let env = TempEnvironment::builder()
        .home_file(
            "Library/Application Support/Cursor/User/settings.json",
            "{}",
        )
        .home_file(
            "Library/Application Support/Cursor/User/keybindings.json",
            "[]",
        )
        .build();

    let ctx = make_ctx(&env);
    let source = env.app_support.join("Cursor");

    commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    // The pack-root directory expanded: the single child of `Cursor/`
    // is `User/`, so the pack contains `_app/Cursor/User/` (a
    // directory whose contents come along).
    let pack_dir = env.dotfiles_root.join("Cursor");
    env.assert_dir_exists(&pack_dir);
    env.assert_regular_file(&pack_dir.join("_app/Cursor/User/settings.json"), "{}");
    env.assert_regular_file(&pack_dir.join("_app/Cursor/User/keybindings.json"), "[]");
    // The expanded child (`User/`) at the original AppSupport
    // location is now a symlink, but the parent `Cursor/` itself
    // stays a real directory.
    assert!(env.fs.is_symlink(&env.app_support.join("Cursor/User")));
    assert!(!env.fs.is_symlink(&source));
}

/// M5 capitalization-heuristic advisory: when a user adopts an
/// AppSupport source whose folder name passes the GUI-app heuristic
/// (`Code`, uppercase), adopt emits a tip pointing at the
/// `app_aliases` ergonomic. The pack tree itself is unaffected — the
/// hint is purely advisory.
#[test]
fn adopt_app_support_emits_capitalization_hint() {
    let env = TempEnvironment::builder()
        .home_file("Library/Application Support/Code/User/settings.json", "{}")
        .build();

    let ctx = make_ctx(&env);
    let source = env.app_support.join("Code/User/settings.json");

    let result = commands::adopt::adopt(
        /*pack_override=*/ None,
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
            .any(|w| w.contains("app_aliases") && w.contains("Code")),
        "expected an `app_aliases` tip in warnings, got: {:?}",
        result.warnings
    );
}

/// Reverse-DNS bundle-ID folders (`com.colliderli.iina`,
/// `dev.warp.Warp-Stable`) get a much better rename suggestion when
/// the M6 brew probe identifies a matching cask: prefer the cask
/// token (`iina`) over the awful whitespace-strip-lowercase fallback
/// (`comcolliderliiina`). Real IINA case from user testing on PR #91.
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "macOS-only enrichment paths")]
fn adopt_app_support_reverse_dns_uses_cask_token_in_tip() {
    let env = TempEnvironment::builder()
        .home_file(
            "Library/Application Support/com.colliderli.iina/input_conf/mine.conf",
            "x",
        )
        .build();

    let runner = Arc::new(CannedRunner::new());
    runner.respond(&["brew", "list", "--cask", "--versions"], "iina 1.4.0\n", 0);
    runner.respond(
        &["brew", "info", "--json=v2", "--cask", "iina"],
        r#"{"casks": [{
            "token": "iina",
            "artifacts": [
                {"app": ["IINA.app"]},
                {"zap": [{"trash": ["~/Library/Application Support/com.colliderli.iina"]}]}
            ]
        }]}"#,
        0,
    );
    let ctx = make_ctx_with_runner(&env, runner);
    let source = env
        .app_support
        .join("com.colliderli.iina/input_conf/mine.conf");

    let result = commands::adopt::adopt(
        /*pack_override=*/ None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    let tip = result
        .warnings
        .iter()
        .find(|w| w.contains("app_aliases"))
        .unwrap_or_else(|| panic!("expected an app_aliases tip, got: {:?}", result.warnings));

    // The good outcome: tip suggests `iina` as the rename target.
    assert!(
        tip.contains("renaming the pack to `iina`"),
        "expected cask-token-based rename suggestion (`iina`), got: {tip}"
    );
    // And explicitly NOT the whitespace-strip-lowercase fallback,
    // which would be `comcolliderliiina` for this folder.
    assert!(
        !tip.contains("comcolliderliiina"),
        "rename suggestion fell back to lowercase mangling instead of cask token: {tip}"
    );
    // The tip credits the cask so the user knows where the
    // recommendation came from.
    assert!(
        tip.contains("matches homebrew cask"),
        "tip should credit the cask source, got: {tip}"
    );
}

/// When no installed cask matches the folder, the tip falls back to
/// the original whitespace-strip-lowercase suggestion. Pins the
/// fallback so a refactor doesn't accidentally regress the no-cask
/// path (the heuristic still triggers on uppercase folders even when
/// brew has nothing to say).
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "macOS-only enrichment paths")]
fn adopt_app_support_falls_back_to_lowercase_when_no_cask_match() {
    // `Tinkerbell` — uppercase enough to trigger the heuristic, but
    // no real cask owns it, so the brew probe returns empty and the
    // tip falls back to the lowercase suggestion.
    let env = TempEnvironment::builder()
        .home_file("Library/Application Support/Tinkerbell/settings.json", "{}")
        .build();

    let runner = Arc::new(CannedRunner::new());
    runner.respond(&["brew", "list", "--cask", "--versions"], "", 0);
    let ctx = make_ctx_with_runner(&env, runner);
    let source = env.app_support.join("Tinkerbell/settings.json");

    let result = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    let tip = result
        .warnings
        .iter()
        .find(|w| w.contains("app_aliases"))
        .unwrap_or_else(|| panic!("expected an app_aliases tip, got: {:?}", result.warnings));

    // Fallback suggestion: lowercased pack name (no spaces here, but
    // the casing transformation still applies).
    assert!(
        tip.contains("renaming the pack to `tinkerbell`"),
        "expected fallback rename suggestion, got: {tip}"
    );
    assert!(
        !tip.contains("matches homebrew cask"),
        "tip should not claim a cask match when none exists: {tip}"
    );
}

/// The advisory is suppressed when the user passed `--into <pack>`:
/// they already chose their pack name, so suggesting another one
/// would be noise. The pack used here (`Code`) only exists to satisfy
/// `--into`'s typo-guard requirement.
#[test]
fn adopt_app_support_into_override_suppresses_hint() {
    let env = TempEnvironment::builder()
        .pack("Code")
        .file("placeholder", "")
        .done()
        .home_file("Library/Application Support/Code/User/settings.json", "{}")
        .build();

    let ctx = make_ctx(&env);
    let source = env.app_support.join("Code/User/settings.json");

    let result = commands::adopt::adopt(
        Some("Code"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    assert!(
        !result.warnings.iter().any(|w| w.contains("app_aliases")),
        "expected no app_aliases tip with --into, got: {:?}",
        result.warnings
    );
}

/// Lowercase CLI-tool-style folder names (`nvim`, `helix`, …) don't
/// trigger the heuristic. An XDG adopt of a typical CLI tool stays
/// hint-free.
#[test]
fn adopt_xdg_lowercase_pack_emits_no_hint() {
    let env = TempEnvironment::builder()
        .home_file(".config/nvim/init.lua", "-- nvim")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".config/nvim/init.lua");

    let result = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    assert!(
        !result.warnings.iter().any(|w| w.contains("app_aliases")),
        "expected no app_aliases tip for plain XDG adopt, got: {:?}",
        result.warnings
    );
}

/// Multiple sources whose inference picks different packs is refused
/// (without `--into`); the message names the conflicting candidates.
#[test]
fn adopt_disagreeing_inferred_packs_refused() {
    let env = TempEnvironment::builder()
        .home_file(".config/nvim/init.lua", "-- nvim")
        .home_file(".config/helix/config.toml", "# helix")
        .build();

    let ctx = make_ctx(&env);
    let sources = vec![
        env.home.join(".config/nvim/init.lua"),
        env.home.join(".config/helix/config.toml"),
    ];

    let err = commands::adopt::adopt(None, &sources, false, false, false, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("different packs"),
        "expected disagreement message, got: {msg}"
    );
    assert!(msg.contains("nvim") && msg.contains("helix"));
}

/// Without `--into`, a HOME source can't infer a pack and adopt fails
/// with a hint pointing at `--into`.
#[test]
fn adopt_home_source_without_into_requires_pack() {
    let env = TempEnvironment::builder()
        .home_file(".vimrc", "set nocompatible")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".vimrc");
    let err = commands::adopt::adopt(
        None,
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("--into"), "expected '--into' hint, got: {msg}");
}

#[test]
fn adopt_already_adopted_source_is_skipped() {
    // Direct symlink to pack source — adopt skips with a #44 message
    // pointing the user at `dodot up` to upgrade to the full chain.
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
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    let warning = result
        .warnings
        .iter()
        .find(|w| w.contains("skipped"))
        .unwrap_or_else(|| panic!("expected a skipped warning, got: {:?}", result.warnings));
    assert!(
        warning.contains("direct symlink to pack source"),
        "expected #44 'direct symlink' wording, got: {warning}"
    );
    assert!(
        warning.contains("dodot up vim"),
        "warning should point user at `dodot up vim`, got: {warning}"
    );
    // Source still a symlink, pack file untouched.
    assert!(env.fs.is_symlink(&source));
    env.assert_regular_file(&pack_file, "content");
}

/// Regression for #44: when the source is fully managed (the user
/// symlink points at dodot's data_dir), adopt skips with the original
/// "already managed by dodot" wording — no upgrade needed.
#[test]
fn adopt_fully_managed_source_keeps_original_skip_message() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "content")
        .done()
        .build();

    let ctx = make_ctx(&env);
    // First, deploy normally so user_path goes through the dodot chain.
    // Under #48 the default deploy target is $XDG_CONFIG_HOME/<pack>/<file>.
    commands::up::up(Some(&["vim".into()]), &ctx).unwrap();

    let source = env.home.join(".config/vim/vimrc");
    assert!(env.fs.is_symlink(&source));

    let result = commands::adopt::adopt(
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        false,
        false,
        &ctx,
    )
    .unwrap();

    let warning = result
        .warnings
        .iter()
        .find(|w| w.contains("skipped"))
        .unwrap_or_else(|| panic!("expected a skipped warning, got: {:?}", result.warnings));
    assert!(
        warning.contains("already managed by dodot"),
        "fully-managed case should keep original wording, got: {warning}"
    );
    assert!(
        !warning.contains("direct symlink"),
        "fully-managed case should NOT use the #44 'direct symlink' wording, got: {warning}"
    );
}

/// Regression for #44: `dodot up` auto-replaces a pre-existing regular
/// file whose content is byte-identical to the pack source — no
/// `--force` needed, no conflict reported.
#[test]
fn up_auto_replaces_content_equivalent_pre_existing_file() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("home.gitconfig", "[user]\n  name = test")
        .done()
        // Same content as the pack source.
        .home_file(".gitconfig", "[user]\n  name = test")
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();

    assert_eq!(
        result.message.as_deref(),
        Some("Packs deployed."),
        "no errors expected for content-equivalent file, got: {:?}",
        result.message
    );
    // ~/.gitconfig is now a symlink (the dodot chain), not a regular file.
    let user_path = env.home.join(".gitconfig");
    assert!(
        env.fs.is_symlink(&user_path),
        "user file should now be a symlink"
    );
    // Content reaching the user is unchanged.
    assert_eq!(
        env.fs.read_to_string(&user_path).unwrap(),
        "[user]\n  name = test"
    );
    // And status agrees: deployed, not a conflict.
    let status = commands::status::status(None, &ctx).unwrap();
    let file = &status.packs[0].files[0];
    assert_eq!(file.status, "deployed");
}

/// Regression for #44: `dodot up` still refuses (without `--force`) when
/// the pre-existing file's content differs from the source. The
/// auto-replace only kicks in for content-equivalent files.
#[test]
fn up_still_refuses_content_different_pre_existing_file() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("home.gitconfig", "[user]\n  name = new")
        .done()
        .home_file(".gitconfig", "[user]\n  name = old")
        .build();

    let ctx = make_ctx(&env);
    let result = commands::up::up(None, &ctx).unwrap();

    assert_eq!(
        result.message.as_deref(),
        Some("Packs deployed with errors."),
        "different content should still conflict, got: {:?}",
        result.message
    );
    // Original content preserved.
    env.assert_file_contents(&env.home.join(".gitconfig"), "[user]\n  name = old");
}

/// Regression for #44: `status` does NOT flag a content-equivalent
/// pre-existing file as PendingConflict (since `up` will handle it
/// without `--force`). Stays plain `pending`, no footnote.
#[test]
fn status_does_not_flag_content_equivalent_file_as_conflict() {
    let env = TempEnvironment::builder()
        .pack("git")
        .file("home.gitconfig", "[user]\n  name = test")
        .done()
        .home_file(".gitconfig", "[user]\n  name = test")
        .build();

    let ctx = make_ctx(&env);
    let status = commands::status::status(None, &ctx).unwrap();
    let file = &status.packs[0].files[0];

    assert_eq!(
        file.status, "pending",
        "content-equivalent file should be plain pending (auto-replaceable), got: {}",
        file.status
    );
    assert!(
        file.note_ref.is_none(),
        "no note_ref for auto-replaceable case"
    );
    assert!(
        status.notes.is_empty(),
        "no notes for auto-replaceable case, got: {:?}",
        status.notes
    );
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
        Some("vim"),
        &[std::path::PathBuf::from("./.vimrc")],
        false,
        false,
        false,
        &ctx,
    );
    std::env::set_current_dir(prev_cwd).unwrap();

    result.expect("adopt should accept ./.vimrc when CWD is HOME");
    env.assert_regular_file(&env.dotfiles_root.join("vim/home.vimrc"), "content");
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
        Some("disabled"),
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
        Some("vim"),
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
        Some("target"),
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
    // Two packs would both end up claiming ~/.bashrc after adoption.
    // Using `bashrc` because it's in `force_home` — different packs both
    // deploy it to ~/.bashrc, producing a real cross-pack conflict.
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("bashrc", "existing")
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
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
    env.assert_not_exists(&env.dotfiles_root.join("work/bashrc"));
}

#[test]
fn adopt_deploy_conflict_not_bypassed_by_force() {
    let env = TempEnvironment::builder()
        .pack("unix")
        .file("bashrc", "existing")
        .done()
        .pack("work")
        .file("placeholder", "")
        .done()
        .home_file(".bashrc", "new")
        .build();

    let ctx = make_ctx(&env);
    let source = env.home.join(".bashrc");
    let err = commands::adopt::adopt(
        Some("work"),
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
        Some("vim"),
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
    env.assert_not_exists(&env.dotfiles_root.join("vim/home.vimrc"));
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
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        true, // --no-follow
        false,
        &ctx,
    )
    .unwrap();

    // The pack copy should be a symlink (not a regular file with copied content).
    let pack_copy = env.dotfiles_root.join("vim/home.vimrc");
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
        .file("home.vimrc", "OLD")
        .done()
        .home_file(".vimrc", "NEW")
        .build();

    let source = env.home.join(".vimrc");
    // chmod 000 makes the file unreadable, so the copy phase fails at
    // read-time without tripping preflight (which uses lstat only).
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o000)).unwrap();

    let ctx = make_ctx(&env);
    let result = commands::adopt::adopt(
        Some("vim"),
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
    env.assert_regular_file(&env.dotfiles_root.join("vim/home.vimrc"), "OLD");
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
        Some("vim"),
        std::slice::from_ref(&source),
        false,
        true, // --no-follow
        false,
        &ctx,
    )
    .expect("adopt with --no-follow on a dangling symlink should succeed");

    // The pack copy should itself be a symlink (preserving the dangling link).
    // Post-#48 adopt rename: ~/.dangling → vim/home.dangling.
    let pack_copy = env.dotfiles_root.join("vim/home.dangling");
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
        Some("vim"),
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
    let err = commands::adopt::adopt(Some("vim"), &[], false, false, false, &ctx).unwrap_err();
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
        Some("newpack"),
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
    // Use `home.X` so the deploy targets ~/.X and collides with the
    // home_file fixture (under #48 the default deploy target is
    // $XDG_CONFIG_HOME/<pack>/X, which wouldn't collide).
    let env = TempEnvironment::builder()
        .pack("ghostty")
        .file("home.ghostrc", "theme=dark")
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
    assert_eq!(
        ghostty_file.status_label, "pending",
        "label should be plain 'pending'; the [N] marker lives in a separate column, got: {}",
        ghostty_file.status_label
    );
    let ghostty_note = ghostty_file
        .note_ref
        .expect("ghostty row should carry a note_ref") as usize
        - 1;
    assert_eq!(
        result.notes.len(),
        1,
        "status should have exactly one note, got: {:?}",
        result.notes
    );
    assert!(
        result.notes[ghostty_note].body.contains(".ghostrc"),
        "note should mention the conflicting path, got: {}",
        result.notes[ghostty_note].body
    );
    assert!(
        result.notes[ghostty_note].body.contains("existing file"),
        "note should classify the target (existing file), got: {}",
        result.notes[ghostty_note].body
    );

    // vim has no pre-existing ~/.vimrc — should be plain pending, no note.
    let vim_file = &vim.files[0];
    assert_eq!(
        vim_file.status, "pending",
        "vim/vimrc has no conflict — should be plain pending"
    );
    assert_eq!(vim_file.status_label, "pending");
    assert!(
        vim_file.note_ref.is_none(),
        "vim row should carry no note_ref"
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
        kitty.files[0].note_ref.is_none(),
        "no note_ref for non-conflict"
    );

    let ghostty = result.packs.iter().find(|p| p.name == "ghostty").unwrap();
    assert_eq!(
        ghostty.files[0].status, "pending",
        "non-equivalent symlink should also be plain pending — executor will replace it"
    );
    assert!(
        ghostty.files[0].note_ref.is_none(),
        "no note_ref for non-conflict"
    );
    assert!(
        result.notes.is_empty(),
        "no notes for non-conflict case, got: {:?}",
        result.notes
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

    // Remove the user link (under #48: $XDG_CONFIG_HOME/vim/vimrc).
    let user_path = env.home.join(".config/vim/vimrc");
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

    // Replace user symlink (under #48: $XDG_CONFIG_HOME/vim/vimrc) with a
    // regular file whose content does NOT match source — that's a real
    // conflict (#44 auto-replace would only kick in for matching content).
    let user_path = env.home.join(".config/vim/vimrc");
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

// ── edge cases ─────────────────────────────────────────────

#[test]
fn up_succeeds_after_resolving_conflict() {
    // Set up conflicting packs
    let env = TempEnvironment::builder()
        .pack("pack-a")
        .file("home.aliases", "a")
        .done()
        .pack("pack-b")
        .file("home.aliases", "b")
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
fn up_conflict_with_home_prefix_convention() {
    // pack-a has `home.bashrc` (uses home. convention → ~/.bashrc)
    // pack-b has `bashrc` (in force_home → ~/.bashrc)
    // Same resolved target → conflict.
    let env = TempEnvironment::builder()
        .pack("a")
        .file("home.bashrc", "# pack a")
        .done()
        .pack("b")
        .file("bashrc", "# pack b")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let err = commands::up::up(None, &ctx).unwrap_err();
    assert!(
        matches!(err, crate::DodotError::CrossPackConflict { .. }),
        "home.bashrc and bashrc both resolve to ~/.bashrc: {err}"
    );
}

#[test]
fn up_multiple_simultaneous_conflicts() {
    // Two conflict groups at the same time
    let env = TempEnvironment::builder()
        .pack("a")
        .file("home.aliases", "a-aliases")
        .file("bashrc", "a-bash")
        .done()
        .pack("b")
        .file("home.aliases", "b-aliases")
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
        .file("home.aliases", "a")
        .done()
        .pack("pack-b")
        .file("home.aliases", "b")
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
    // Both packs use `_xdg/nvim/init.lua` (the per-subtree XDG escape
    // hatch — skips the pack name in the path) → both resolve to
    // ~/.config/nvim/init.lua, conflict.
    //
    // (Without `_xdg/`, the new default would namespace each pack
    // under its own dir — `~/.config/nvim-base/...` vs `~/.config/
    // nvim-custom/...` — and they wouldn't collide.)
    let env = TempEnvironment::builder()
        .pack("nvim-base")
        .file("_xdg/nvim/init.lua", "-- base config")
        .done()
        .pack("nvim-custom")
        .file("_xdg/nvim/init.lua", "-- custom config")
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

// ── view-mode / group-mode tests ────────────────────────────

#[test]
fn summary_aggregates_all_deployed_as_deployed() {
    use crate::commands::{DisplayFile, DisplayPack};

    let files = vec![
        DisplayFile {
            name: "a".into(),
            symbol: "➞".into(),
            description: "".into(),
            status: "deployed".into(),
            status_label: "deployed".into(),
            handler: "symlink".into(),
            note_ref: None,
        },
        DisplayFile {
            name: "b".into(),
            symbol: "➞".into(),
            description: "".into(),
            status: "deployed".into(),
            status_label: "deployed".into(),
            handler: "symlink".into(),
            note_ref: None,
        },
    ];
    let pack = DisplayPack::new("vim".into(), files);
    assert_eq!(pack.summary_status, "deployed");
    assert_eq!(pack.summary_count, 2);
}

#[test]
fn summary_rolls_up_error_over_pending_over_deployed() {
    use crate::commands::{DisplayFile, DisplayPack};

    let mk = |status: &str| DisplayFile {
        name: status.into(),
        symbol: "➞".into(),
        description: "".into(),
        status: status.into(),
        status_label: status.into(),
        handler: "symlink".into(),
        note_ref: None,
    };

    // error beats pending beats deployed
    let pack = DisplayPack::new(
        "mixed".into(),
        vec![mk("error"), mk("pending"), mk("deployed")],
    );
    assert_eq!(pack.summary_status, "error");
    assert_eq!(pack.summary_count, 1);

    // broken rolls into error bucket
    let pack = DisplayPack::new("b".into(), vec![mk("broken"), mk("deployed")]);
    assert_eq!(pack.summary_status, "error");

    // stale and warning roll into pending bucket
    let pack = DisplayPack::new("s".into(), vec![mk("stale"), mk("deployed")]);
    assert_eq!(pack.summary_status, "pending");
    let pack = DisplayPack::new("w".into(), vec![mk("warning"), mk("deployed")]);
    assert_eq!(pack.summary_status, "pending");

    // count counts only files in the winning bucket
    let pack = DisplayPack::new(
        "counts".into(),
        vec![
            mk("error"),
            mk("broken"),
            mk("pending"),
            mk("pending"),
            mk("deployed"),
        ],
    );
    assert_eq!(pack.summary_status, "error");
    assert_eq!(pack.summary_count, 2);
}

#[test]
fn short_mode_renders_one_line_per_pack_with_count() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .pack("nvim")
        .file("init.lua", "x")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.view_mode = crate::commands::ViewMode::Short;
    let result = commands::status::status(None, &ctx).unwrap();

    let output = render::render("pack-status", &result, OutputMode::Text).unwrap();

    // Short mode: one line per pack, count + status word, no per-file rows
    assert!(output.contains("vim"), "output: {output}");
    assert!(output.contains("nvim"), "output: {output}");
    assert!(output.contains("(1) pending"), "output: {output}");
    assert!(
        !output.contains("vimrc"),
        "short mode should not render individual files: {output}"
    );
    assert!(
        !output.contains("init.lua"),
        "short mode should not render individual files: {output}"
    );
}

#[test]
fn by_status_groups_packs_under_banners() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .pack("nvim")
        .file("init.lua", "x")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.group_mode = crate::commands::GroupMode::Status;
    let result = commands::status::status(None, &ctx).unwrap();

    let output = render::render("pack-status", &result, OutputMode::Text).unwrap();

    // All packs pending, so only the Pending banner appears
    assert!(output.contains("Pending Packs"), "output: {output}");
    assert!(
        !output.contains("Deployed Packs"),
        "no deployed packs — deployed banner should be hidden: {output}"
    );
    assert!(
        !output.contains("Error Packs"),
        "no error packs — error banner should be hidden: {output}"
    );
    // Pack names still render within the group
    assert!(output.contains("vim"), "output: {output}");
    assert!(output.contains("nvim"), "output: {output}");
}

// ── probe ──────────────────────────────────────────────────────

#[test]
fn probe_summary_lists_available_subcommands() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    let result = commands::probe::summary(&ctx).unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(output.contains("deployment-map"), "output:\n{output}");
    assert!(output.contains("show-data-dir"), "output:\n{output}");
}

#[test]
fn probe_deployment_map_renders_rows_after_up() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build();
    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let result = commands::probe::deployment_map(&ctx).unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(output.contains("vim"), "output:\n{output}");
    assert!(output.contains("shell"), "output:\n{output}");
    assert!(output.contains("aliases.sh"), "output:\n{output}");
}

#[test]
fn probe_deployment_map_empty_state_shows_hint() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    let result = commands::probe::deployment_map(&ctx).unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        output.contains("nothing deployed"),
        "empty probe should point the user at `dodot up`; got:\n{output}"
    );
}

#[test]
fn probe_show_data_dir_renders_tree_with_sizes() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build();
    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let result = commands::probe::show_data_dir(&ctx, 4).unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(output.contains("packs"), "output:\n{output}");
    assert!(output.contains("vim"), "output:\n{output}");
    assert!(output.contains("shell"), "output:\n{output}");
    // Tree should use box-drawing glyphs somewhere.
    assert!(
        output.contains("├") || output.contains("└"),
        "expected branch glyphs in tree; got:\n{output}"
    );
}

#[test]
fn probe_deployment_map_json_mode_is_kind_tagged() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    let result = commands::probe::deployment_map(&ctx).unwrap();
    let output = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["kind"], "deployment-map");
    assert!(parsed["entries"].is_array());
}

// ── probe shell-init Phase 3 (--runs / --history) ─────────────────

fn write_fake_profile(env: &TempEnvironment, name: &str, lines: &[&str]) {
    let dir = env.paths.probes_shell_init_dir();
    env.fs.mkdir_all(&dir).unwrap();
    let mut content =
        String::from("# columns\tphase\tpack\thandler\ttarget\tstart_t\tend_t\texit_status\n");
    for l in lines {
        content.push_str(l);
        content.push('\n');
    }
    env.fs
        .write_file(&dir.join(name), content.as_bytes())
        .unwrap();
}

#[test]
fn probe_shell_init_aggregate_renders_percentile_table() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    // Three fake profiles with the same target; verify p50/p95/max
    // surface in the rendered text.
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &["source\tvim\tshell\t/x/aliases.sh\t1.000000\t1.000100\t0"],
    );
    write_fake_profile(
        &env,
        "profile-1714000002-1-1.tsv",
        &["source\tvim\tshell\t/x/aliases.sh\t1.000000\t1.000200\t0"],
    );
    write_fake_profile(
        &env,
        "profile-1714000003-1-1.tsv",
        &["source\tvim\tshell\t/x/aliases.sh\t1.000000\t1.000300\t0"],
    );
    let result = commands::probe::shell_init_aggregate(&ctx, 5).unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        output.contains("aggregate"),
        "header missing; got:\n{output}"
    );
    assert!(output.contains("aliases.sh"), "row missing; got:\n{output}");
    assert!(output.contains("3/3"), "seen-label missing; got:\n{output}");
}

#[test]
fn probe_shell_init_aggregate_warns_when_fewer_runs_than_requested() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &["source\tvim\tshell\t/x.sh\t1.000000\t1.000100\t0"],
    );
    // Asked for 10, only 1 on disk.
    let result = commands::probe::shell_init_aggregate(&ctx, 10).unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        output.contains("requested 10"),
        "expected mismatch warning; got:\n{output}"
    );
}

#[test]
fn probe_shell_init_aggregate_empty_state_shows_hint() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    let result = commands::probe::shell_init_aggregate(&ctx, 5).unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        output.contains("no profiles yet"),
        "expected empty hint; got:\n{output}"
    );
}

#[test]
fn probe_shell_init_history_renders_one_row_per_run_newest_first() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    // Three profiles with distinct timestamps in their filenames.
    write_fake_profile(
        &env,
        "profile-1714000000-1-1.tsv",
        &["source\tvim\tshell\t/a.sh\t1.000000\t1.000100\t0"],
    );
    write_fake_profile(
        &env,
        "profile-1714003600-1-1.tsv",
        &["source\tvim\tshell\t/a.sh\t1.000000\t1.000200\t1"],
    );
    write_fake_profile(
        &env,
        "profile-1714007200-1-1.tsv",
        &["source\tvim\tshell\t/a.sh\t1.000000\t1.000300\t0"],
    );
    let result = commands::probe::shell_init_history(&ctx, 50).unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(output.contains("history"), "header missing; got:\n{output}");
    // Date stamps from the timestamps (1714000000 ≈ 2024-04-24 23:06 UTC).
    assert!(
        output.contains("2024-04-24"),
        "date missing; got:\n{output}"
    );
    // Three rendered rows; ordering check via JSON because the text
    // template's column padding makes substring offsets fragile.
    let json = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let rows = parsed["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    // Newest unix_ts first, oldest last (descending).
    let timestamps: Vec<u64> = rows
        .iter()
        .map(|r| r["unix_ts"].as_u64().unwrap_or(0))
        .collect();
    assert_eq!(timestamps, vec![1714007200, 1714003600, 1714000000]);
    // Middle row had a non-zero exit_status.
    assert_eq!(rows[1]["failed_entries"].as_u64().unwrap(), 1);
    assert_eq!(rows[0]["failed_entries"].as_u64().unwrap(), 0);
    assert_eq!(rows[2]["failed_entries"].as_u64().unwrap(), 0);
}

#[test]
fn probe_shell_init_history_empty_state_shows_hint() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    let result = commands::probe::shell_init_history(&ctx, 50).unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        output.contains("no profiles yet"),
        "expected empty hint; got:\n{output}"
    );
}

#[test]
fn probe_shell_init_aggregate_json_is_kind_tagged() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    let result = commands::probe::shell_init_aggregate(&ctx, 1).unwrap();
    let output = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["kind"], "shell-init-aggregate");
    assert!(parsed["rows"].is_array());
    assert!(parsed["requested_runs"].is_number());
}

#[test]
fn probe_shell_init_history_json_is_kind_tagged() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    let result = commands::probe::shell_init_history(&ctx, 1).unwrap();
    let output = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["kind"], "shell-init-history");
    assert!(parsed["rows"].is_array());
}

#[test]
fn probe_shell_init_filter_json_is_kind_tagged() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    let result = commands::probe::shell_init_filter(&ctx, "vim", 5).unwrap();
    let output = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["kind"], "shell-init-filter");
    assert!(parsed["targets"].is_array());
    assert_eq!(parsed["filter_pack"], "vim");
}

#[test]
fn probe_shell_init_errors_json_is_kind_tagged() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    let result = commands::probe::shell_init_errors(&ctx, 5).unwrap();
    let output = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["kind"], "shell-init-errors");
    assert!(parsed["targets"].is_array());
}

// ── probe shell-init: staleness banner (#59) ────────────────

/// Plant a `last-up-at` marker at the given unix timestamp so tests
/// don't depend on real wall-clock writes.
fn write_last_up_marker_at(env: &TempEnvironment, ts: u64) {
    env.fs.mkdir_all(env.paths.data_dir()).unwrap();
    env.fs
        .write_file(&env.paths.last_up_path(), ts.to_string().as_bytes())
        .unwrap();
}

/// Profile filenames encode the unix timestamp. Profile pre-dates the
/// last `up`, so the staleness banner must fire.
#[test]
fn probe_shell_init_banner_when_profile_predates_last_up() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);

    write_fake_profile(
        &env,
        "profile-1714000000-1-1.tsv",
        &["source\tvim\tshell\t/x/aliases.sh\t1.000000\t1.000100\t0"],
    );
    // Up happened one hour after the profile.
    write_last_up_marker_at(&env, 1714003600);

    let result = commands::probe::shell_init(&ctx).unwrap();
    let json = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["stale"], true);

    let text = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        text.contains("warning:"),
        "expected staleness banner, got:\n{text}"
    );
    // Banner mentions both timestamps so the user can verify the comparison.
    assert!(
        text.contains("2024-04-24") && text.contains("2024-04-25"),
        "banner should reference both capture and up timestamps, got:\n{text}"
    );
    assert!(
        text.contains("capture a fresh profile"),
        "banner should explain the remediation, got:\n{text}"
    );
}

#[test]
fn probe_shell_init_no_banner_when_profile_postdates_last_up() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);

    // Up first, profile after — the user already opened a shell, so
    // the displayed profile reflects the post-up state. No banner.
    write_last_up_marker_at(&env, 1714000000);
    write_fake_profile(
        &env,
        "profile-1714003600-1-1.tsv",
        &["source\tvim\tshell\t/x/aliases.sh\t1.000000\t1.000100\t0"],
    );

    let result = commands::probe::shell_init(&ctx).unwrap();
    let json = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["stale"], false);

    let text = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        !text.contains("warning:"),
        "no banner expected when profile is fresh, got:\n{text}"
    );
}

#[test]
fn probe_shell_init_no_banner_when_no_last_up_marker() {
    // Profile exists but `up` has never run on this machine — we have
    // nothing to compare against, so the safe default is "no banner".
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);

    write_fake_profile(
        &env,
        "profile-1714000000-1-1.tsv",
        &["source\tvim\tshell\t/x/aliases.sh\t1.000000\t1.000100\t0"],
    );

    let result = commands::probe::shell_init(&ctx).unwrap();
    let text = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        !text.contains("warning:"),
        "no banner without an up marker, got:\n{text}"
    );
}

#[test]
fn probe_shell_init_no_banner_when_no_profile() {
    // Marker exists, but there's no profile yet (e.g. user just ran
    // first `up`). The empty-state hint is enough; no warning.
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_last_up_marker_at(&env, 1714000000);

    let result = commands::probe::shell_init(&ctx).unwrap();
    let text = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        !text.contains("warning:"),
        "no banner when there's no profile to flag, got:\n{text}"
    );
}

#[test]
fn probe_shell_init_aggregate_banner_when_newest_predates_last_up() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);

    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &["source\tvim\tshell\t/x.sh\t1.000000\t1.000100\t0"],
    );
    write_fake_profile(
        &env,
        "profile-1714000002-1-1.tsv",
        &["source\tvim\tshell\t/x.sh\t1.000000\t1.000200\t0"],
    );
    // Up happened after the newest profile.
    write_last_up_marker_at(&env, 1714000003);

    let result = commands::probe::shell_init_aggregate(&ctx, 5).unwrap();
    let json = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["stale"], true);

    let text = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        text.contains("warning:"),
        "aggregate view should show banner, got:\n{text}"
    );
}

#[test]
fn probe_shell_init_history_banner_when_newest_predates_last_up() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);

    write_fake_profile(
        &env,
        "profile-1714000000-1-1.tsv",
        &["source\tvim\tshell\t/x.sh\t1.000000\t1.000100\t0"],
    );
    write_fake_profile(
        &env,
        "profile-1714003600-1-1.tsv",
        &["source\tvim\tshell\t/x.sh\t1.000000\t1.000200\t0"],
    );
    // Up after the newest history row.
    write_last_up_marker_at(&env, 1714007200);

    let result = commands::probe::shell_init_history(&ctx, 50).unwrap();
    let json = render::render("probe", &result, OutputMode::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["stale"], true);

    let text = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        text.contains("warning:"),
        "history view should show banner, got:\n{text}"
    );
}

// ── shell-init filter (positional pack[/file] drill-down) ──

fn write_fake_errors_log(env: &TempEnvironment, profile_name: &str, body: &str) {
    let dir = env.paths.probes_shell_init_dir();
    env.fs.mkdir_all(&dir).unwrap();
    let stem = profile_name.trim_end_matches(".tsv");
    let path = dir.join(format!("{stem}.errors.log"));
    let mut content = String::from("# dodot shell-init errors v1\n");
    content.push_str(body);
    env.fs.write_file(&path, content.as_bytes()).unwrap();
}

#[test]
fn probe_shell_init_filter_pack_only_lists_each_target_in_pack() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &[
            "source\tgpg\tshell\t/p/gpg/env.sh\t1.0\t1.001\t1",
            "source\tgpg\tshell\t/p/gpg/aliases.sh\t1.0\t1.001\t0",
            "source\tvim\tshell\t/p/vim/aliases.sh\t1.0\t1.001\t0",
        ],
    );
    let result =
        commands::probe::shell_init_filter(&ctx, "gpg", commands::probe::DEFAULT_FILTER_RUNS)
            .unwrap();
    let view = match result {
        commands::probe::ProbeResult::ShellInitFilter(v) => v,
        other => panic!("expected ShellInitFilter, got {other:?}"),
    };
    assert_eq!(view.filter_pack, "gpg");
    assert!(view.filter_filename.is_none());
    assert_eq!(view.targets.len(), 2, "expected both gpg targets");
    let names: Vec<&str> = view
        .targets
        .iter()
        .map(|t| t.display_target.as_str())
        .collect();
    assert!(names.contains(&"env.sh"));
    assert!(names.contains(&"aliases.sh"));
}

#[test]
fn probe_shell_init_filter_with_filename_narrows_to_single_target() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &[
            "source\tgpg\tshell\t/p/gpg/env.sh\t1.0\t1.001\t1",
            "source\tgpg\tshell\t/p/gpg/aliases.sh\t1.0\t1.001\t0",
        ],
    );
    let result = commands::probe::shell_init_filter(
        &ctx,
        "gpg/env.sh",
        commands::probe::DEFAULT_FILTER_RUNS,
    )
    .unwrap();
    let view = match result {
        commands::probe::ProbeResult::ShellInitFilter(v) => v,
        other => panic!("expected ShellInitFilter, got {other:?}"),
    };
    assert_eq!(view.filter_pack, "gpg");
    assert_eq!(view.filter_filename.as_deref(), Some("env.sh"));
    assert_eq!(view.targets.len(), 1);
    assert_eq!(view.targets[0].display_target, "env.sh");
    assert_eq!(view.targets[0].failure_count, 1);
}

#[test]
fn probe_shell_init_filter_attaches_captured_stderr_to_matching_run() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &["source\tgpg\tshell\t/p/gpg/env.sh\t1.0\t1.001\t1"],
    );
    write_fake_errors_log(
        &env,
        "profile-1714000001-1-1.tsv",
        "@@\t/p/gpg/env.sh\t1\nfirst error line\nsecond error line\n",
    );
    let result = commands::probe::shell_init_filter(
        &ctx,
        "gpg/env.sh",
        commands::probe::DEFAULT_FILTER_RUNS,
    )
    .unwrap();
    let view = match result {
        commands::probe::ProbeResult::ShellInitFilter(v) => v,
        other => panic!("expected ShellInitFilter, got {other:?}"),
    };
    assert_eq!(view.targets.len(), 1);
    assert_eq!(view.targets[0].runs.len(), 1);
    assert_eq!(
        view.targets[0].runs[0].stderr_lines,
        vec!["first error line", "second error line"]
    );
}

#[test]
fn probe_shell_init_filter_runs_are_newest_first() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    // Three runs with monotonically increasing timestamps in the
    // filename — the filter view should display them newest first
    // (most recent run at the top of the per-target block).
    for ts in [1714000001u64, 1714000002, 1714000003] {
        write_fake_profile(
            &env,
            &format!("profile-{ts}-1-1.tsv"),
            &["source\tgpg\tshell\t/p/gpg/env.sh\t1.0\t1.001\t0"],
        );
    }
    let result = commands::probe::shell_init_filter(
        &ctx,
        "gpg/env.sh",
        commands::probe::DEFAULT_FILTER_RUNS,
    )
    .unwrap();
    let view = match result {
        commands::probe::ProbeResult::ShellInitFilter(v) => v,
        other => panic!("expected ShellInitFilter, got {other:?}"),
    };
    let runs = &view.targets[0].runs;
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].profile_filename, "profile-1714000003-1-1.tsv");
    assert_eq!(runs[2].profile_filename, "profile-1714000001-1-1.tsv");
}

#[test]
fn probe_shell_init_filter_renders_with_template() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &["source\tgpg\tshell\t/p/gpg/env.sh\t1.0\t1.001\t1"],
    );
    write_fake_errors_log(
        &env,
        "profile-1714000001-1-1.tsv",
        "@@\t/p/gpg/env.sh\t1\nboom\n",
    );
    let result = commands::probe::shell_init_filter(
        &ctx,
        "gpg/env.sh",
        commands::probe::DEFAULT_FILTER_RUNS,
    )
    .unwrap();
    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        output.contains("Shell-init filter"),
        "header missing:\n{output}"
    );
    assert!(output.contains("env.sh"), "target missing:\n{output}");
    assert!(output.contains("exit 1"), "exit code missing:\n{output}");
    assert!(
        output.contains("boom"),
        "captured stderr missing:\n{output}"
    );
}

#[test]
fn probe_shell_init_filter_supports_nested_subpaths() {
    // A target deployed under a subdirectory (e.g. `pack/sub/dir/x.sh`)
    // should be matchable both by basename (`x.sh`) and by subpath
    // (`sub/dir/x.sh`). The latter is the disambiguator when two files
    // share a basename.
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &[
            "source\tgpg\tshell\t/p/gpg/sub/dir/env.sh\t1.0\t1.001\t1",
            "source\tgpg\tshell\t/p/gpg/other/env.sh\t1.0\t1.001\t0",
        ],
    );

    // Subpath filter narrows to the matching nested file only.
    let result = commands::probe::shell_init_filter(
        &ctx,
        "gpg/sub/dir/env.sh",
        commands::probe::DEFAULT_FILTER_RUNS,
    )
    .unwrap();
    let view = match result {
        commands::probe::ProbeResult::ShellInitFilter(v) => v,
        other => panic!("expected ShellInitFilter, got {other:?}"),
    };
    assert_eq!(view.targets.len(), 1);
    assert_eq!(view.targets[0].target, "/p/gpg/sub/dir/env.sh");

    // Bare basename still matches both nested files.
    let result_basename = commands::probe::shell_init_filter(
        &ctx,
        "gpg/env.sh",
        commands::probe::DEFAULT_FILTER_RUNS,
    )
    .unwrap();
    let view_basename = match result_basename {
        commands::probe::ProbeResult::ShellInitFilter(v) => v,
        other => panic!("expected ShellInitFilter, got {other:?}"),
    };
    assert_eq!(view_basename.targets.len(), 2);
}

#[test]
fn probe_shell_init_filter_basename_does_not_partial_match() {
    // Boundary check: `env.sh` filter must not match `nvenv.sh`.
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &[
            "source\tnv\tshell\t/p/nv/nvenv.sh\t1.0\t1.001\t0",
            "source\tnv\tshell\t/p/nv/env.sh\t1.0\t1.001\t0",
        ],
    );
    let result =
        commands::probe::shell_init_filter(&ctx, "nv/env.sh", commands::probe::DEFAULT_FILTER_RUNS)
            .unwrap();
    let view = match result {
        commands::probe::ProbeResult::ShellInitFilter(v) => v,
        other => panic!("expected ShellInitFilter, got {other:?}"),
    };
    assert_eq!(view.targets.len(), 1);
    assert_eq!(view.targets[0].target, "/p/nv/env.sh");
}

#[test]
fn probe_shell_init_filter_empty_when_no_match() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &["source\tvim\tshell\t/p/vim/aliases.sh\t1.0\t1.001\t0"],
    );
    let result =
        commands::probe::shell_init_filter(&ctx, "missing", commands::probe::DEFAULT_FILTER_RUNS)
            .unwrap();
    let view = match result {
        commands::probe::ProbeResult::ShellInitFilter(v) => v,
        other => panic!("expected ShellInitFilter, got {other:?}"),
    };
    assert!(view.targets.is_empty());
    assert_eq!(view.runs_examined, 1);
}

// ── shell-init --errors-only ─────────────────────────────────────

#[test]
fn probe_shell_init_errors_only_keeps_only_failed_runs() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &[
            "source\tgpg\tshell\t/p/gpg/env.sh\t1.0\t1.001\t1",
            "source\tvim\tshell\t/p/vim/aliases.sh\t1.0\t1.001\t0",
        ],
    );
    let result =
        commands::probe::shell_init_errors(&ctx, commands::probe::DEFAULT_FILTER_RUNS).unwrap();
    let view = match result {
        commands::probe::ProbeResult::ShellInitErrors(v) => v,
        other => panic!("expected ShellInitErrors, got {other:?}"),
    };
    // vim/aliases.sh succeeded — must not appear. Only gpg/env.sh.
    assert_eq!(view.targets.len(), 1);
    assert_eq!(view.targets[0].display_target, "env.sh");
    assert_eq!(view.targets[0].failure_count, 1);
}

#[test]
fn probe_shell_init_errors_only_sorts_by_failure_count_desc() {
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    // Three profiles: target A fails twice, B fails once.
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &[
            "source\ta\tshell\t/p/a.sh\t1.0\t1.001\t1",
            "source\tb\tshell\t/p/b.sh\t1.0\t1.001\t1",
        ],
    );
    write_fake_profile(
        &env,
        "profile-1714000002-1-1.tsv",
        &["source\ta\tshell\t/p/a.sh\t1.0\t1.001\t1"],
    );
    let result =
        commands::probe::shell_init_errors(&ctx, commands::probe::DEFAULT_FILTER_RUNS).unwrap();
    let view = match result {
        commands::probe::ProbeResult::ShellInitErrors(v) => v,
        other => panic!("expected ShellInitErrors, got {other:?}"),
    };
    assert_eq!(view.targets.len(), 2);
    assert_eq!(
        view.targets[0].pack, "a",
        "most-broken target must come first"
    );
    assert_eq!(view.targets[0].failure_count, 2);
    assert_eq!(view.targets[1].pack, "b");
    assert_eq!(view.targets[1].failure_count, 1);
}

#[test]
fn probe_shell_init_errors_only_clean_window_says_so() {
    // Only successful runs in the window — view shows 0 targets and
    // the renderer surfaces a cheerful "no failed sources" line.
    let env = TempEnvironment::builder().build();
    let ctx = make_ctx(&env);
    write_fake_profile(
        &env,
        "profile-1714000001-1-1.tsv",
        &["source\tvim\tshell\t/p/aliases.sh\t1.0\t1.001\t0"],
    );
    let result =
        commands::probe::shell_init_errors(&ctx, commands::probe::DEFAULT_FILTER_RUNS).unwrap();
    match &result {
        commands::probe::ProbeResult::ShellInitErrors(v) => {
            assert!(v.targets.is_empty());
            assert_eq!(v.runs_examined, 1);
        }
        other => panic!("expected ShellInitErrors, got {other:?}"),
    }

    let output = render::render("probe", &result, OutputMode::Text).unwrap();
    assert!(
        output.contains("no failed sources"),
        "clean-window message missing:\n{output}"
    );
}

// ── up command misc ─────────────────────────────────────────────

#[test]
fn up_writes_last_up_marker() {
    // The marker is what the staleness check compares against, so the
    // up command must always leave one behind on a successful run.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .build();
    let ctx = make_ctx(&env);

    assert!(
        !env.fs.exists(&env.paths.last_up_path()),
        "marker should not exist before first up"
    );
    commands::up::up(None, &ctx).unwrap();
    assert!(
        env.fs.exists(&env.paths.last_up_path()),
        "marker should be written by up"
    );

    let raw = env.fs.read_to_string(&env.paths.last_up_path()).unwrap();
    let parsed: u64 = raw.trim().parse().expect("marker should be a unix ts");
    // Sanity: post-2023.
    assert!(parsed > 1_700_000_000, "ts should look recent: {parsed}");
}

// ── deployment map (written on up/down alongside the init script) ──

#[test]
fn up_writes_deployment_map() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .file("bin/tool", "#!/bin/sh")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    env.assert_exists(&env.paths.deployment_map_path());
    let content = env
        .fs
        .read_to_string(&env.paths.deployment_map_path())
        .unwrap();
    assert!(content.starts_with("# dodot deployment map v1"));
    assert!(
        content.contains("vim\tshell\tsymlink\t"),
        "expected a vim/shell row; content:\n{content}"
    );
    assert!(
        content.contains("vim\tpath\tsymlink\t"),
        "expected a vim/path row; content:\n{content}"
    );
}

#[test]
fn down_refreshes_deployment_map_to_empty() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build();

    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();
    // Precondition: map has a row.
    let content_before = env
        .fs
        .read_to_string(&env.paths.deployment_map_path())
        .unwrap();
    assert!(content_before.contains("aliases.sh"));

    commands::down::down(None, &ctx).unwrap();

    let content_after = env
        .fs
        .read_to_string(&env.paths.deployment_map_path())
        .unwrap();
    // Header stays; data rows are gone.
    assert!(content_after.starts_with("# dodot deployment map v1"));
    assert!(
        !content_after.contains("aliases.sh"),
        "map should be empty after down; got:\n{content_after}"
    );
}

#[test]
fn up_dry_run_does_not_touch_deployment_map() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("aliases.sh", "alias vi=vim")
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.dry_run = true;
    commands::up::up(None, &ctx).unwrap();

    // Map file should not have been written for a dry-run.
    env.assert_not_exists(&env.paths.deployment_map_path());
}

#[test]
fn by_status_folds_ignored_packs_into_ignored_group() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "x")
        .done()
        .pack("disabled")
        .file("stuff", "x")
        .ignored()
        .done()
        .build();

    let mut ctx = make_ctx(&env);
    ctx.group_mode = crate::commands::GroupMode::Status;
    let result = commands::status::status(None, &ctx).unwrap();

    let output = render::render("pack-status", &result, OutputMode::Text).unwrap();

    assert!(output.contains("Ignored Packs"), "output: {output}");
    assert!(output.contains("disabled"), "output: {output}");
    assert!(output.contains("Pending Packs"), "output: {output}");
}

// ── M6: probe::app + advisory probes ─────────────────────────

/// `dodot probe app <pack>` collects every folder this pack would
/// route to (alias, force_app, _app/), checks each against the
/// app-support root, and (with mocked brew + mdls) enriches the
/// matching cask token, .app bundle, and bundle ID. The probe is
/// advisory — resolver state is unchanged.
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "macOS-only enrichment paths")]
fn probe_app_collects_alias_force_and_underscore_entries() {
    let env = TempEnvironment::builder()
        .pack("vscode")
        .file("settings.json", "{}")
        .file("_app/Cursor/User/keys.json", "[]")
        .file("Code/User/extra.json", "{}")
        .config("[symlink.app_aliases]\nvscode = \"VSCodeAliased\"\n")
        .done()
        .build();
    // Pre-create one of the target folders so `target_exists` differs
    // across rows and the test pins the existence column.
    env.fs.mkdir_all(&env.app_support.join("Cursor")).unwrap();

    let runner = Arc::new(CannedRunner::new());
    runner.respond(
        &["brew", "list", "--cask", "--versions"],
        "cursor 0.42.0\n",
        0,
    );
    runner.respond(
        &["brew", "info", "--json=v2", "--cask", "cursor"],
        r#"{"casks": [{
            "token": "cursor",
            "installed": "0.42.0",
            "artifacts": [
                {"app": ["Cursor.app"]},
                {"zap": [{"trash": [
                    "~/Library/Application Support/Cursor",
                    "~/Library/Preferences/com.todesktop.Cursor.plist"
                ]}]}
            ]
        }]}"#,
        0,
    );
    runner.respond(
        &[
            "mdls",
            "-name",
            "kMDItemCFBundleIdentifier",
            "/Applications/Cursor.app",
        ],
        "kMDItemCFBundleIdentifier = \"com.todesktop.Cursor\"\n",
        0,
    );
    let ctx = make_ctx_with_runner(&env, runner);

    let result = commands::probe::app("vscode", false, &ctx).unwrap();
    let view = match result {
        commands::probe::ProbeResult::App(v) => v,
        other => panic!("expected App variant, got {other:?}"),
    };
    assert_eq!(view.pack, "vscode");
    assert!(view.macos);

    // Three folders: VSCodeAliased (alias), Code (force_app default
    // includes Code), Cursor (_app/ subtree).
    let folders: Vec<&str> = view.entries.iter().map(|e| e.folder.as_str()).collect();
    assert!(folders.contains(&"VSCodeAliased"), "folders: {folders:?}");
    assert!(folders.contains(&"Code"), "folders: {folders:?}");
    assert!(folders.contains(&"Cursor"), "folders: {folders:?}");

    // Cursor is the only pre-created folder → exists; others missing.
    let cursor_row = view.entries.iter().find(|e| e.folder == "Cursor").unwrap();
    assert!(cursor_row.target_exists);
    // `cask` is always an *installed* token (matching iterates only
    // `brew list --cask --versions`), so a `Some` value implies
    // installed — there's no separate field for it any more.
    assert_eq!(cursor_row.cask.as_deref(), Some("cursor"));
    assert_eq!(cursor_row.app_bundle.as_deref(), Some("Cursor.app"));
    assert_eq!(
        cursor_row.bundle_id.as_deref(),
        Some("com.todesktop.Cursor")
    );

    // Sibling-adoption suggestions surfaced from cask zap.
    assert!(
        view.suggested_adoptions
            .iter()
            .any(|s| s.contains("Cursor.plist")),
        "suggested adoptions: {:?}",
        view.suggested_adoptions
    );
}

/// `dodot probe app ..` (or any other path-traversing input) must
/// not let `pack_path` traversal escape the dotfiles root. Probe
/// validates that `pack_name` is a single-component path before
/// passing it to `Pather::pack_path`. Regression for review feedback
/// on PR #91.
#[test]
fn probe_app_rejects_path_traversal_input() {
    let env = TempEnvironment::builder().build();
    let runner = Arc::new(CannedRunner::new());
    let ctx = make_ctx_with_runner(&env, runner);

    for evil in ["..", "foo/../bar", "../sibling", "/abs/path"] {
        let result = commands::probe::app(evil, false, &ctx).unwrap();
        let view = match result {
            commands::probe::ProbeResult::App(v) => v,
            other => panic!("expected App variant, got {other:?}"),
        };
        // Empty-but-named view: the pack name echoes back, but no
        // entries are surfaced (filesystem traversal was skipped).
        assert_eq!(view.pack, evil, "input echoed back unchanged");
        assert!(
            view.entries.is_empty(),
            "path-traversing input must not produce entries: got {:?}",
            view.entries
        );
    }
}

/// On non-macOS, probe::app still produces a useful view (folder
/// existence under the collapsed app-support root) but skips brew /
/// Spotlight enrichment entirely. `macos` is `false`.
#[test]
fn probe_app_non_macos_returns_minimal_view() {
    if cfg!(target_os = "macos") {
        // The cfg! gate inside probe::app keys off the host. On macOS
        // hosts we can't simulate the Linux path; skip rather than
        // contort the test fixture.
        return;
    }
    let env = TempEnvironment::builder()
        .pack("vscode")
        .file("Code/User/foo", "{}")
        .done()
        .build();
    let runner = Arc::new(CannedRunner::new());
    let ctx = make_ctx_with_runner(&env, runner);

    let result = commands::probe::app("vscode", false, &ctx).unwrap();
    let view = match result {
        commands::probe::ProbeResult::App(v) => v,
        other => panic!("expected App variant, got {other:?}"),
    };
    assert!(!view.macos);
    // No brew enrichment.
    for entry in &view.entries {
        assert!(entry.cask.is_none(), "row: {entry:?}");
        assert!(entry.app_bundle.is_none(), "row: {entry:?}");
        assert!(entry.bundle_id.is_none(), "row: {entry:?}");
    }
}

/// `up` / `status` emit a missing-target hint when an app-support
/// folder doesn't exist on disk and a brew cask matches. macOS-only
/// — the orchestration gate is the same `cfg!(target_os = "macos")`.
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
fn plan_pack_emits_missing_target_hint_with_cask_enrichment() {
    use crate::packs::orchestration;
    use crate::packs::Pack;

    let env = TempEnvironment::builder()
        .pack("vscode")
        .file("settings.json", "{}")
        .config("[symlink.app_aliases]\nvscode = \"Code\"\n")
        .done()
        .build();
    // `Code` folder is intentionally absent — the hint should fire.
    assert!(!env.app_support.join("Code").exists());

    let runner = Arc::new(CannedRunner::new());
    runner.respond(
        &["brew", "list", "--cask", "--versions"],
        "visual-studio-code 1.95.0\n",
        0,
    );
    runner.respond(
        &["brew", "info", "--json=v2", "--cask", "visual-studio-code"],
        r#"{"casks": [{
            "token": "visual-studio-code",
            "artifacts": [
                {"app": ["Visual Studio Code.app"]},
                {"zap": [{"trash": ["~/Library/Application Support/Code"]}]}
            ]
        }]}"#,
        0,
    );
    let ctx = make_ctx_with_runner(&env, runner);

    // The planner uses cache_only=true to keep `up`/`status` fast —
    // an empty cache produces the unenriched message. Pre-warm the
    // cache by calling info_cask once (the on-demand path that may
    // spawn brew). Production users get the same warm cache via
    // `dodot probe app` or `dodot adopt`.
    let cache_dir = ctx.paths.probes_brew_cache_dir();
    let _ = crate::probe::brew::info_cask(
        "visual-studio-code",
        &cache_dir,
        crate::probe::brew::now_secs_unix(),
        ctx.fs.as_ref(),
        ctx.command_runner.as_ref(),
    );

    // Synthesize a Pack matching the on-disk pack we built.
    let pack_path = env.dotfiles_root.join("vscode");
    let pack_config = ctx.config_manager.config_for_pack(&pack_path).unwrap();
    let pack = Pack {
        name: "vscode".into(),
        display_name: "vscode".into(),
        path: pack_path,
        config: pack_config.to_handler_config(),
    };

    let plan = orchestration::plan_pack(&pack, &ctx, true).unwrap();
    let hint = plan.warnings.iter().find(|w| w.contains("Code"));
    assert!(
        hint.is_some(),
        "expected missing-target hint mentioning `Code`; got {:?}",
        plan.warnings
    );
    let hint_text = hint.unwrap();
    assert!(
        hint_text.contains("visual-studio-code"),
        "expected cask-enriched hint, got: {hint_text}"
    );
    // Per review feedback: the cask is installed (we read it from
    // `brew list`), so the message must NOT claim it isn't installed.
    assert!(
        !hint_text.contains("isn't installed"),
        "hint should not falsely claim the cask is uninstalled, got: {hint_text}"
    );
}
