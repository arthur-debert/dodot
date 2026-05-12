//! Integration tests for command API.
//!
//! Big tests — adopt, probe + shell-init, gating — live in sibling
//! files so each per-command suite stays under its own roof and
//! `cargo test commands::tests::adopt::` can target a single
//! command's coverage without compiling the rest of the suite.
//! Shared test fixtures live in [`mod@support`].

mod adopt;
mod gating;
mod probe;
mod support;

#[allow(unused_imports)]
use std::sync::Arc;

use crate::commands;
#[allow(unused_imports)]
use crate::config::ConfigManager;
#[allow(unused_imports)]
use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
use crate::fs::Fs;
#[allow(unused_imports)]
use crate::packs::orchestration::ExecutionContext;
#[allow(unused_imports)]
use crate::paths::Pather;
use crate::render;
use crate::testing::TempEnvironment;
#[allow(unused_imports)]
use crate::Result;
use standout_render::OutputMode;

use support::make_ctx;
#[allow(unused_imports)]
use support::{make_ctx_with_runner, CannedRunner};

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
fn status_marks_readme_and_license_as_skipped() {
    // Files matched by `mappings.skip` (defaults: README, LICENSE,
    // CHANGELOG, …) should appear in status with handler "skip"
    // and status "skipped" rather than being silently dropped or
    // routed to the symlink catchall.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .file("README.md", "# vim pack")
        .file("license", "MIT")
        .file("CHANGELOG", "v1: initial")
        .done()
        .build();

    let ctx = make_ctx(&env);
    let result = commands::status::status(None, &ctx).unwrap();

    let pack = &result.packs[0];
    let by_name: std::collections::HashMap<&str, &commands::DisplayFile> =
        pack.files.iter().map(|f| (f.name.as_str(), f)).collect();

    let readme = by_name.get("README.md").expect("README.md in status");
    assert_eq!(readme.handler, "skip");
    assert_eq!(readme.status, "skipped");
    assert_eq!(readme.status_label, "skipped");

    let license = by_name.get("license").expect("license in status");
    assert_eq!(license.handler, "skip", "case-insensitive match");

    let changelog = by_name.get("CHANGELOG").expect("CHANGELOG in status");
    assert_eq!(changelog.handler, "skip");

    let vimrc = by_name.get("vimrc").expect("vimrc in status");
    assert_eq!(vimrc.handler, "symlink");
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

/// Regression: status must follow the planner's intent expansion for
/// escape-prefix directories (`_home/`, `_xdg/`, `_app/`, `_lib/`).
///
/// Before this was fixed, status iterated raw scanner matches and
/// rendered `_app` as a single row resolving to the default rule
/// (`$XDG_CONFIG_HOME/<pack>/_app`) — a path the planner never deploys
/// to. Because the data link for that bogus target never exists,
/// verification reported "pending" indefinitely, even after a
/// successful `up`. Meanwhile the real leaf files (deployed under
/// `<app_support>/...` per the `_app/<rest>` rule) didn't appear in
/// status output at all.
///
/// The fix has status drive its deployable rows from
/// `orchestration::plan_pack` (the same intents the executor runs),
/// not from raw matches. This test pins that contract end-to-end:
/// after `up`, status must show the per-leaf row deployed at the
/// app-support path — never an `_app` row pointing at
/// `~/.config/<pack>/_app pending`.
#[test]
fn up_then_status_expands_app_escape_prefix_per_file() {
    let env = TempEnvironment::builder()
        .pack("iina")
        .file("_app/com.colliderli.iina/input_conf/mine.conf", "# keys")
        .done()
        .build();

    let ctx = make_ctx(&env);

    // up must deploy the leaf to the app-support path the planner's
    // `_app/<rest>` rule resolves to.
    commands::up::up(None, &ctx).unwrap();
    let deployed_user_link = env
        .app_support
        .join("com.colliderli.iina/input_conf/mine.conf");
    assert!(
        env.fs.is_symlink(&deployed_user_link),
        "up should have created the user link at {}",
        deployed_user_link.display()
    );

    // status must render that deployment, not a bogus `_app` row.
    let result = commands::status::status(None, &ctx).unwrap();
    let pack = result
        .packs
        .iter()
        .find(|p| p.name == "iina")
        .expect("iina pack must appear in status");

    // No row should claim the bogus default-rule target. If status
    // emits an `_app` row at all, it must not pretend the deploy
    // landed under `~/.config/iina/_app`.
    let bogus_target_row = pack
        .files
        .iter()
        .find(|f| f.handler == "symlink" && f.description.contains(".config/iina/_app"));
    assert!(
        bogus_target_row.is_none(),
        "status must not surface the default-rule `_app` target; \
         escape-prefix dirs expand per-file. got: {:?}",
        pack.files
            .iter()
            .map(|f| (&f.name, &f.description, &f.status))
            .collect::<Vec<_>>()
    );

    // The leaf file must appear as a deployed symlink row, with the
    // target pointing somewhere under the app-support root.
    let leaf = pack
        .files
        .iter()
        .find(|f| {
            f.handler == "symlink"
                && f.description
                    .contains("com.colliderli.iina/input_conf/mine.conf")
        })
        .unwrap_or_else(|| {
            panic!(
                "expected a deployed leaf row for the `_app/.../mine.conf` file; got: {:?}",
                pack.files
                    .iter()
                    .map(|f| (&f.name, &f.description, &f.status))
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(
        leaf.status, "deployed",
        "leaf row must be deployed after up; row: {leaf:?}"
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

// ── cfprefsd drift marker (#109) ────────────────────────────

#[cfg(target_os = "macos")]
#[test]
fn up_writes_cfprefsd_marker_on_first_run_with_plists() {
    // First-ever `up`: no previous last-up marker, so any plist
    // file in an active pack counts as "drifted." The marker
    // must land for the post-up prompt to fire.
    let env = TempEnvironment::builder()
        .pack("mac-defaults")
        .file("com.example.app.plist", "<?xml?><plist></plist>")
        .done()
        .build();
    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let marker = ctx.paths.data_dir().join("cfprefsd-needs-invalidation");
    assert!(
        ctx.fs.exists(&marker),
        "marker should land on the first up that deploys a plist"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn up_does_not_write_cfprefsd_marker_when_pack_has_no_plists() {
    // Pack contains no plist files → the cfprefsd prompt has
    // nothing to invalidate; the marker must stay absent.
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    let ctx = make_ctx(&env);
    commands::up::up(None, &ctx).unwrap();

    let marker = ctx.paths.data_dir().join("cfprefsd-needs-invalidation");
    assert!(
        !ctx.fs.exists(&marker),
        "marker must not appear when no plists are present"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn up_with_pack_filter_does_not_write_cfprefsd_marker_for_unrelated_pack_plists() {
    // Pack A contains a plist; pack B has only non-plist files.
    // Running `dodot up` filtered to pack B must NOT drop the
    // cfprefsd marker — the user's command didn't touch any plist.
    let env = TempEnvironment::builder()
        .pack("mac-defaults")
        .file("com.example.app.plist", "<?xml?><plist></plist>")
        .done()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    let ctx = make_ctx(&env);
    let filter = vec!["vim".to_string()];
    commands::up::up(Some(&filter), &ctx).unwrap();

    let marker = ctx.paths.data_dir().join("cfprefsd-needs-invalidation");
    assert!(
        !ctx.fs.exists(&marker),
        "drift detection must respect the pack filter — \
         a plist in an unrelated pack should not trigger the marker"
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
