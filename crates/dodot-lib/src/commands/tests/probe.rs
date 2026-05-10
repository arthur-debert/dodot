//! Integration tests for the `probe` command family (probe summary, deployment-map, shell-init in all modes, and the macOS app-advisory probes).

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

    let plan = orchestration::plan_pack(&pack, &ctx, crate::preprocessing::PreprocessMode::Active)
        .unwrap();
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
