//! Entry points and helpers for the `probe shell-init` family.
//!
//! Five public entry points produce the five `ShellInit*` `ProbeResult`
//! variants:
//!
//! - [`shell_init`] — most recent complete profile, grouped by (pack, handler),
//!   plus (by default) the live hook-line trace: one report, two
//!   halves, because the user running it is asking "why isn't my
//!   shell init working" and the halves answer that together
//! - [`shell_init_aggregate`] — percentile stats across last N runs
//! - [`shell_init_history`] — one summary row per recent profile
//! - [`shell_init_filter`] — drill-down by `<pack>[/<file>]`
//! - [`shell_init_errors`] — non-zero-exit entries across the window
//!
//! Only [`shell_init`] may carry the trace, and only it may spawn a
//! shell; every other view — and every other command — stays passive
//! (INS01 §9 still binds).

use crate::commands::probe::types::{
    ProbeResult, ShellInitAggregateRow, ShellInitAggregateView, ShellInitErrorsView,
    ShellInitFilterRun, ShellInitFilterTarget, ShellInitFilterView, ShellInitGroup,
    ShellInitHistoryRow, ShellInitHistoryView, ShellInitRow, ShellInitTraceView, ShellInitView,
};
use crate::packs::orchestration::ExecutionContext;
use crate::probe::{
    aggregate_profiles, group_profile, parse_unix_ts_from_filename, read_last_up_marker,
    read_latest_profile, read_recent_profiles, summarize_history, AggregatedTarget, GroupedProfile,
    HistoryEntry,
};
use crate::Result;

/// Render the most recent complete shell-init profile, with the live
/// hook-line trace when `trace` is true.
///
/// When no complete profile has been written yet (fresh install,
/// profiling disabled, no shell started since the last `up`, or only
/// interrupted profiles exist), returns a "no data" view with
/// `has_profile = false`. The template distinguishes the interrupted
/// case from the ordinary empty state.
///
/// `trace` is the caller's suppression switch (`--no-trace`, or a
/// `<file>` argument having routed to the filter view instead): when
/// false the report is the recorded timings alone and nothing is
/// spawned. When true, [`trace_view`] appends the live half — which
/// still spawns only under [`crate::shell::ProbePolicy::Gated`], so
/// no non-production context can reach a real shell.
pub fn shell_init(ctx: &ExecutionContext, trace: bool) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;

    let profile_opt = read_latest_profile(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let latest_profile_incomplete = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), 1)?
        .first()
        .is_some_and(|p| !p.complete);
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());
    let last_up_when = last_up_ts.map(format_unix_ts).unwrap_or_default();

    let mut view = match profile_opt {
        Some(profile) => {
            let grouped = group_profile(&profile);
            let profile_ts = parse_unix_ts_from_filename(&profile.filename);
            let stale = is_stale(profile_ts, last_up_ts);
            ShellInitView {
                filename: profile.filename.clone(),
                shell: profile.shell.clone(),
                profiling_enabled,
                has_profile: true,
                latest_profile_incomplete: false,
                groups: shell_init_groups(&grouped),
                user_total_us: grouped.user_total_us,
                framing_us: grouped.framing_us,
                total_us: grouped.total_us,
                profiles_dir,
                stale,
                profile_when: format_unix_ts(profile_ts),
                last_up_when,
                trace: None,
            }
        }
        None => ShellInitView {
            filename: String::new(),
            shell: String::new(),
            profiling_enabled,
            has_profile: false,
            latest_profile_incomplete,
            groups: Vec::new(),
            user_total_us: 0,
            framing_us: 0,
            total_us: 0,
            profiles_dir,
            stale: false,
            profile_when: String::new(),
            last_up_when,
            trace: None,
        },
    };
    if trace {
        view.trace = Some(trace_view(ctx));
    }

    Ok(ProbeResult::ShellInit(view))
}

/// Decide whether a profile timestamp predates the last `dodot up`.
/// Returns false when either timestamp is unknown — we never warn on
/// guesswork, only when we have both reference points.
fn is_stale(profile_ts: u64, last_up_ts: Option<u64>) -> bool {
    matches!(last_up_ts, Some(last) if profile_ts > 0 && profile_ts < last)
}

// ── The live half: PATH at the hook line ──────────────────────────

/// How long a `<found dodot> --version` gets before its group is
/// killed — the winner at the hook line may be arbitrary and must not
/// be able to hang the report.
const VERSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Build the live half of the report: resolve the shell, the rc, and
/// the hook; spawn the trace; judge what the hook line actually
/// reaches — what `dodot` resolves to on the traced `PATH` for the
/// `eval` form, whether the script *that line names* is there for the
/// file-source form.
///
/// Nothing to trace is not an error (an unsupported shell, no rc, no
/// hook — each reports plainly and spawns nothing), and a trace that
/// could not run degrades to a "could not trace" line rather than
/// failing the command: the recorded half must survive a hostile rc.
/// A hook line dodot cannot read gets the same treatment one level
/// down — an unanswered question, never an answered one about the
/// wrong file.
///
/// The spawn is announced before it runs and rides the INS01 envelope
/// via [`crate::shell::trace::run_trace`].
fn trace_view(ctx: &ExecutionContext) -> ShellInitTraceView {
    use crate::shell::activation;
    use crate::shell::rc;
    use crate::shell::trace::{self, HookForm, SourcedScript, TraceError};

    let fs = ctx.fs.as_ref();
    let home = ctx.paths.home_dir();

    // Which shell — dodot only guesses rc files for bash and zsh.
    let Some(shell) = ctx.shell_env.hookup_shell() else {
        let reason = match ctx.shell_env.shell.as_deref() {
            Some(other) => format!("{other} is not a shell dodot can trace (bash and zsh only)"),
            None => "$SHELL is not set".to_string(),
        };
        return skipped_trace(reason, String::new(), String::new());
    };
    let shell_name = shell.as_str().to_string();
    // `hookup_shell()` returned Some, so the shell path is set.
    let shell_path = ctx.shell_env.shell.clone().unwrap_or_default();

    // Which rc — same ladder as `install` and the INS01 probe, so the
    // trace never measures one shell while naming another's rc file.
    let target = rc::resolve_rc(fs, home, Some(shell), &ctx.shell_env, None);
    let rc_display = rc::display_home_relative(target.nominal(), home);
    if !target.exists {
        return skipped_trace(
            format!("no rc file at {rc_display} — nothing to trace"),
            shell_name,
            rc_display.clone(),
        );
    }

    // Which line — the record to read is addressable, never searched.
    let Ok(rc_text) = fs.read_to_string(&target.path) else {
        return skipped_trace(
            format!("could not read {rc_display} — nothing to trace"),
            shell_name,
            rc_display.clone(),
        );
    };
    let Some(hook) = trace::find_hook(&rc_text, home) else {
        return skipped_trace(
            format!("no dodot hook in {rc_display} — run `dodot install --write` to add one"),
            shell_name,
            rc_display.clone(),
        );
    };
    let hook_line = hook.line;

    // Only a context that may measure gets to spawn; every
    // non-production context leaves the policy at `Never`.
    let Some(timeout) = ctx.shell_probe.timeout() else {
        return untraced(
            "shell tracing is unavailable in this context".to_string(),
            shell_name,
            rc_display,
            hook_line,
        );
    };

    eprintln!("{}", trace::announcement(shell));
    let request = trace::TraceRequest {
        shell_path: std::path::Path::new(&shell_path),
        shell,
        home,
        // The same `$ZDOTDIR` the rc ladder resolved this target with,
        // so the traced startup and the named rc file can never come
        // from two different readings of the environment.
        zdotdir: ctx.shell_env.zdotdir.as_deref(),
        rc_nominal: target.nominal(),
        rc_resolved: &target.path,
        hook_line,
        timeout,
    };
    let run = match trace::run_trace(fs, &request) {
        Ok(run) => run,
        Err(TraceError::TimedOut) => {
            return untraced(
                "your shell did not finish starting up in time".to_string(),
                shell_name,
                rc_display,
                hook_line,
            )
        }
        Err(TraceError::SpawnFailed(e)) => {
            return untraced(
                format!("could not run your shell ({e})"),
                shell_name,
                rc_display,
                hook_line,
            )
        }
        Err(TraceError::RcUnreadable(e)) => {
            return untraced(
                format!("could not read {rc_display} ({e})"),
                shell_name,
                rc_display,
                hook_line,
            )
        }
        Err(TraceError::FallbackUnfaithful(e)) => {
            return untraced(
                format!("could not reproduce your shell's startup files ({e})"),
                shell_name,
                rc_display,
                hook_line,
            )
        }
    };

    let record = trace::record_at(&run.records, &[target.nominal(), &target.path], hook_line);
    let verdict = match record {
        None => trace::TraceVerdict::HookNeverRan,
        Some(record) => match hook.form {
            HookForm::Eval => {
                // The verdict is an identity comparison against the
                // running binary. Without it there is no comparison to
                // make — only a false "different dodot" report, since
                // no resolved path equals a path we do not know.
                let Ok(running_exe) = std::env::current_exe() else {
                    return untraced(
                        "could not identify the running dodot binary".to_string(),
                        shell_name,
                        rc_display,
                        hook_line,
                    );
                };
                trace::judge_eval_hook(fs, record, &running_exe, &version_by_running)
            }
            // The file-source hook involves no PATH: the equivalent
            // check is which dodot wrote the script *that line*
            // sources — the path from the hook, never from
            // `paths.init_script_path()`, and the identity read out of
            // the script rather than assumed from its existence.
            HookForm::FileSource(SourcedScript::Path(script)) => trace::judge_file_source_hook(
                fs,
                script,
                // The generation `dodot up` last wrote: what a sound
                // hook's script should be carrying.
                activation::read_script_generation(fs, ctx.paths.as_ref()),
                activation::running_version(),
            ),
            HookForm::FileSource(SourcedScript::Unresolved { raw }) => {
                trace::TraceVerdict::ScriptUnresolved { raw }
            }
        },
    };
    verdict_trace_view(
        ctx,
        verdict,
        shell_name,
        rc_display,
        hook_line,
        run.used_fallback,
    )
}

/// A trace view for "nothing to trace": stated plainly, nothing
/// spawned.
fn skipped_trace(reason: String, shell: String, rc: String) -> ShellInitTraceView {
    ShellInitTraceView {
        status: "skipped".into(),
        status_class: "dim",
        shell,
        rc,
        hook_line: 0,
        verdict: String::new(),
        headline: reason,
        detail_lines: Vec::new(),
        used_fallback: false,
    }
}

/// A trace view for "wanted to trace but could not": the recorded
/// half stands alone, labeled honestly.
fn untraced(reason: String, shell: String, rc: String, hook_line: usize) -> ShellInitTraceView {
    ShellInitTraceView {
        status: "untraced".into(),
        status_class: "warning",
        shell,
        rc,
        hook_line,
        verdict: String::new(),
        headline: format!("could not trace — {reason}"),
        detail_lines: Vec::new(),
        used_fallback: false,
    }
}

/// Learn a binary's version by running `<binary> --version` under the
/// same envelope (timeout, process-group kill) as every other spawn —
/// the winner at the hook line is arbitrary and gets no more trust
/// than the user's rc does.
fn version_by_running(binary: &std::path::Path) -> Option<String> {
    use crate::shell::probe::{spawn_captured, SpawnOutcome};
    let mut command = std::process::Command::new(binary);
    command.arg("--version");
    match spawn_captured(command, VERSION_TIMEOUT) {
        SpawnOutcome::Finished(capture) => {
            crate::shell::trace::parse_version_output(&capture.stdout)
        }
        _ => None,
    }
}

/// Flatten a verdict into the display strings the template and the
/// JSON view share.
fn verdict_trace_view(
    ctx: &ExecutionContext,
    verdict: crate::shell::trace::TraceVerdict,
    shell: String,
    rc: String,
    hook_line: usize,
    used_fallback: bool,
) -> ShellInitTraceView {
    use crate::shell::activation::running_version;
    use crate::shell::rc::display_home_relative;
    use crate::shell::trace::{Resolution, SkipReason, TraceVerdict};

    let home = ctx.paths.home_dir();
    let show = |p: &std::path::Path| display_home_relative(p, home);
    let skips = |resolution: &Resolution| -> Vec<String> {
        resolution
            .notable_skips()
            .map(|c| match &c.skipped {
                Some(SkipReason::DanglingSymlink { target: Some(t) }) => {
                    format!(
                        "passed over {} — dangling symlink → {}",
                        show(&c.path),
                        show(t)
                    )
                }
                Some(SkipReason::DanglingSymlink { target: None }) => {
                    format!("passed over {} — dangling symlink", show(&c.path))
                }
                Some(SkipReason::NotExecutable) => {
                    format!("passed over {} — not executable", show(&c.path))
                }
                Some(SkipReason::MissingDir) => {
                    format!(
                        "passed over {} — its directory does not exist",
                        show(&c.path)
                    )
                }
                _ => String::new(),
            })
            .filter(|l| !l.is_empty())
            .collect()
    };

    let (tag, status_class, headline, detail_lines) = match verdict {
        TraceVerdict::HookNeverRan => (
            "hook-never-ran",
            "error",
            format!("the hook at {rc}:{hook_line} never ran in a fresh shell"),
            vec![
                "it may sit inside a branch that did not run, or the rc exits or execs away \
                 before reaching it"
                    .to_string(),
            ],
        ),
        TraceVerdict::Unresolvable { path, resolution } => {
            let mut details = skips(&resolution);
            // An empty PATH is still a search — of the working
            // directory, which is what an empty entry means to the
            // shell — so it is reported as empty, not as unsearched.
            details.push(if path.is_empty() {
                "PATH is empty at that line".to_string()
            } else {
                format!("PATH there: {path}")
            });
            (
                "unresolvable",
                "error",
                format!("`dodot` is not resolvable at {rc}:{hook_line}"),
                details,
            )
        }
        TraceVerdict::DifferentBinary {
            found,
            found_version,
            running,
            resolution,
        } => {
            let found_version = found_version.unwrap_or_else(|| "version unknown".into());
            let mut details = vec![
                format!(
                    "at {rc}:{hook_line}, `dodot` resolves to {} ({found_version})",
                    show(&found)
                ),
                format!("you are running {} ({})", show(&running), running_version()),
            ];
            details.extend(skips(&resolution));
            (
                "different-binary",
                "error",
                "your shells load a different dodot than the one running now".to_string(),
                details,
            )
        }
        TraceVerdict::RunningBinary { path } => (
            "running-binary",
            "deployed",
            format!(
                "`dodot` at {rc}:{hook_line} resolves to the running binary — the hookup is sound"
            ),
            vec![show(&path)],
        ),
        TraceVerdict::ScriptMissing { script } => (
            "script-missing",
            "error",
            format!(
                "the init script sourced at {rc}:{hook_line} does not exist — run `dodot up` to \
                 regenerate it"
            ),
            vec![show(&script)],
        ),
        TraceVerdict::ScriptPresent { script } => (
            "script-ok",
            "deployed",
            format!(
                "the init script sourced at {rc}:{hook_line} was written by the running dodot — \
                 the hookup is sound"
            ),
            vec![show(&script)],
        ),
        // The file-source twin of `different-binary`: the hook sources
        // a real dodot init script, written by a dodot that is not
        // this one.
        TraceVerdict::ScriptSkewed { script, version } => (
            "script-skewed",
            "error",
            format!("the init script sourced at {rc}:{hook_line} was written by a different dodot"),
            vec![
                format!("{} says dodot {version}", show(&script)),
                format!("you are running {}", running_version()),
                "your shells load that script, not the one `dodot up` maintains — re-point the \
                 hook, or run `dodot install --write` to rewrite it"
                    .to_string(),
            ],
        ),
        TraceVerdict::ScriptStale {
            script,
            found,
            expected,
        } => (
            "script-stale",
            "error",
            format!(
                "the init script sourced at {rc}:{hook_line} is older than the one `dodot up` \
                 maintains"
            ),
            vec![
                format!(
                    "{} is generation {found}; the current one is {expected}",
                    show(&script)
                ),
                "it is a copy left behind — re-point the hook, or run `dodot install --write`"
                    .to_string(),
            ],
        ),
        TraceVerdict::ScriptUnverified { script, reason } => (
            "script-unverified",
            "warning",
            format!(
                "dodot could not tell which dodot wrote the script sourced at {rc}:{hook_line}"
            ),
            vec![format!("{} — {reason}", show(&script))],
        ),
        // Not a verdict: the line names its script in a form dodot
        // will not resolve without interpreting shell, and a guess
        // here would be a claim about a file the hook may not source.
        TraceVerdict::ScriptUnresolved { raw } => (
            "script-unresolved",
            "warning",
            format!("dodot could not tell which file the hook at {rc}:{hook_line} sources"),
            vec![
                format!("the line sources {raw}"),
                "dodot expands only an absolute path or one under `$HOME` — check that file \
                 yourself, or run `dodot install --write` to wire the hook it can read"
                    .to_string(),
            ],
        ),
    };
    ShellInitTraceView {
        status: "verdict".into(),
        status_class,
        shell,
        rc,
        hook_line,
        verdict: tag.into(),
        headline,
        detail_lines,
        used_fallback,
    }
}

fn shell_init_groups(grouped: &GroupedProfile) -> Vec<ShellInitGroup> {
    grouped
        .groups
        .iter()
        .map(|g| ShellInitGroup {
            pack: g.pack.clone(),
            handler: g.handler.clone(),
            rows: g
                .rows
                .iter()
                .map(|r| ShellInitRow {
                    target: short_target(&r.target),
                    duration_us: r.duration_us,
                    duration_label: humanize_us(r.duration_us),
                    exit_status: r.exit_status,
                    status_class: if r.exit_status == 0 {
                        "deployed"
                    } else {
                        "error"
                    },
                })
                .collect(),
            group_total_us: g.group_total_us,
            group_total_label: humanize_us(g.group_total_us),
        })
        .collect()
}

/// Display-friendly basename for a target path. The fully-qualified
/// path is in the on-disk profile already; the rendered table is
/// narrow.
fn short_target(target: &str) -> String {
    std::path::Path::new(target)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.to_string())
}

/// Compact human duration: "0 µs" / "1.2 ms" / "350 ms" / "1.4 s".
pub fn humanize_us(us: u64) -> String {
    if us < 1_000 {
        format!("{us} µs")
    } else if us < 1_000_000 {
        format!("{:.1} ms", us as f64 / 1_000.0)
    } else {
        format!("{:.2} s", us as f64 / 1_000_000.0)
    }
}

/// Aggregate the last `runs` profiles into per-target percentile stats.
///
/// The CLI applies a default of 10 when the user passes `--runs`
/// without a value (see `clap`'s `default_missing_value` in
/// `dodot-cli/src/main.rs`); this function takes the resolved count
/// directly so it stays useful from external callers (tests, custom
/// harnesses) that pick their own N.
pub fn shell_init_aggregate(ctx: &ExecutionContext, runs: usize) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;
    let profiles = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), runs)?;
    // read_recent_profiles returns newest-first, so the first entry's
    // filename is the most recent capture.
    let latest_profile_ts = profiles
        .first()
        .map(|p| parse_unix_ts_from_filename(&p.filename))
        .unwrap_or(0);
    let view = aggregate_profiles(&profiles);
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());

    Ok(ProbeResult::ShellInitAggregate(ShellInitAggregateView {
        runs: view.runs,
        requested_runs: runs,
        profiling_enabled,
        profiles_dir,
        rows: view.targets.into_iter().map(into_aggregate_row).collect(),
        stale: is_stale(latest_profile_ts, last_up_ts),
        latest_profile_when: format_unix_ts(latest_profile_ts),
        last_up_when: last_up_ts.map(format_unix_ts).unwrap_or_default(),
    }))
}

fn into_aggregate_row(t: AggregatedTarget) -> ShellInitAggregateRow {
    ShellInitAggregateRow {
        pack: t.pack,
        handler: t.handler,
        target: short_target(&t.target),
        p50_label: humanize_us(t.p50_us),
        p95_label: humanize_us(t.p95_us),
        max_label: humanize_us(t.max_us),
        p50_us: t.p50_us,
        p95_us: t.p95_us,
        max_us: t.max_us,
        seen_label: format!("{}/{}", t.runs_seen, t.runs_total),
        runs_seen: t.runs_seen,
        runs_total: t.runs_total,
    }
}

/// Match a profile target path against a filename-or-subpath filter.
///
/// Returns true when:
/// - the filter is a bare basename (`env.sh`) and `target`'s last path
///   component equals it, or
/// - the filter is a subpath (`subdir/env.sh`) and `target` ends with
///   that subpath at a path boundary.
///
/// The boundary check (`/{filter}` suffix) prevents `env.sh` from
/// matching `nvenv.sh` or other filenames that happen to end with the
/// same characters.
fn target_matches_filter(target: &str, filter: &str) -> bool {
    if !filter.contains('/') {
        return std::path::Path::new(target)
            .file_name()
            .is_some_and(|s| s == std::ffi::OsStr::new(filter));
    }
    target.ends_with(&format!("/{filter}")) || target == filter
}

/// Render the filtered drill-down view for a `<pack>[/<file>]` filter.
///
/// `runs` controls how many recent profiles are examined; the caller
/// passes [`crate::commands::probe::DEFAULT_FILTER_RUNS`] unless it has
/// a specific reason to look further or fewer.
pub fn shell_init_filter(ctx: &ExecutionContext, filter: &str, runs: usize) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());
    let last_up_when = last_up_ts.map(format_unix_ts).unwrap_or_default();

    // Filter parsing: `pack` or `pack/file`. Trim a leading `./` and a
    // trailing `/` defensively so users can paste tab-completed paths.
    let trimmed = filter.trim().trim_start_matches("./").trim_end_matches('/');
    let (filter_pack, filter_filename) = match trimmed.split_once('/') {
        Some((p, f)) if !p.is_empty() && !f.is_empty() => (p.to_string(), Some(f.to_string())),
        _ => (trimmed.to_string(), None),
    };

    let profiles = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), runs)?;
    let latest_profile_ts = profiles
        .first()
        .map(|p| parse_unix_ts_from_filename(&p.filename))
        .unwrap_or(0);

    // Bucket per `(pack, handler, target)`. Order: targets sorted by
    // path so output is stable; runs within each target stay newest-
    // first (matching the input slice order).
    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<(String, String, String), Vec<ShellInitFilterRun>> = BTreeMap::new();

    for profile in &profiles {
        let when = format_unix_ts(parse_unix_ts_from_filename(&profile.filename));
        for entry in &profile.entries {
            if entry.pack != filter_pack {
                continue;
            }
            if let Some(name) = &filter_filename {
                if !target_matches_filter(&entry.target, name) {
                    continue;
                }
            }
            let stderr_lines: Vec<String> = profile
                .errors
                .iter()
                .find(|er| er.target == entry.target)
                .map(|er| {
                    er.message
                        .trim_end()
                        .lines()
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            buckets
                .entry((
                    entry.pack.clone(),
                    entry.handler.clone(),
                    entry.target.clone(),
                ))
                .or_default()
                .push(ShellInitFilterRun {
                    when: when.clone(),
                    duration_us: entry.duration_us,
                    duration_label: humanize_us(entry.duration_us),
                    exit_status: entry.exit_status,
                    status_class: if entry.exit_status == 0 {
                        "deployed"
                    } else {
                        "error"
                    },
                    stderr_lines,
                    profile_filename: profile.filename.clone(),
                });
        }
    }

    let targets: Vec<ShellInitFilterTarget> = buckets
        .into_iter()
        .map(|((pack, handler, target), runs_vec)| {
            let display_target = std::path::Path::new(&target)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| target.clone());
            let failure_count = runs_vec.iter().filter(|r| r.exit_status != 0).count();
            ShellInitFilterTarget {
                target,
                display_target,
                pack,
                handler,
                runs: runs_vec,
                failure_count,
            }
        })
        .collect();

    Ok(ProbeResult::ShellInitFilter(ShellInitFilterView {
        profiling_enabled,
        profiles_dir,
        filter: filter.trim().to_string(),
        filter_pack,
        filter_filename,
        runs_examined: profiles.len(),
        targets,
        stale: is_stale(latest_profile_ts, last_up_ts),
        latest_profile_when: format_unix_ts(latest_profile_ts),
        last_up_when,
    }))
}

/// Render the cross-history errors view.
///
/// Scans the last `runs` profiles, keeps only entries with non-zero
/// exit status, groups them by target, and orders by failure count
/// (most-broken first). The `runs` parameter follows the same window
/// convention as [`shell_init_filter`].
pub fn shell_init_errors(ctx: &ExecutionContext, runs: usize) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());
    let last_up_when = last_up_ts.map(format_unix_ts).unwrap_or_default();

    let profiles = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), runs)?;
    let latest_profile_ts = profiles
        .first()
        .map(|p| parse_unix_ts_from_filename(&p.filename))
        .unwrap_or(0);

    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<(String, String, String), Vec<ShellInitFilterRun>> = BTreeMap::new();

    for profile in &profiles {
        let when = format_unix_ts(parse_unix_ts_from_filename(&profile.filename));
        for entry in &profile.entries {
            if entry.exit_status == 0 {
                continue;
            }
            let stderr_lines: Vec<String> = profile
                .errors
                .iter()
                .find(|er| er.target == entry.target)
                .map(|er| {
                    er.message
                        .trim_end()
                        .lines()
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            buckets
                .entry((
                    entry.pack.clone(),
                    entry.handler.clone(),
                    entry.target.clone(),
                ))
                .or_default()
                .push(ShellInitFilterRun {
                    when: when.clone(),
                    duration_us: entry.duration_us,
                    duration_label: humanize_us(entry.duration_us),
                    exit_status: entry.exit_status,
                    status_class: "error",
                    stderr_lines,
                    profile_filename: profile.filename.clone(),
                });
        }
    }

    let mut targets: Vec<ShellInitFilterTarget> = buckets
        .into_iter()
        .map(|((pack, handler, target), runs_vec)| {
            let display_target = std::path::Path::new(&target)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| target.clone());
            let failure_count = runs_vec.len();
            ShellInitFilterTarget {
                target,
                display_target,
                pack,
                handler,
                runs: runs_vec,
                failure_count,
            }
        })
        .collect();

    // Sort: most-broken first, with a stable (pack, handler, target)
    // tiebreaker so two targets with the same failure count don't swap
    // positions across runs.
    targets.sort_by(|a, b| {
        b.failure_count
            .cmp(&a.failure_count)
            .then_with(|| a.pack.cmp(&b.pack))
            .then_with(|| a.handler.cmp(&b.handler))
            .then_with(|| a.target.cmp(&b.target))
    });

    Ok(ProbeResult::ShellInitErrors(ShellInitErrorsView {
        profiling_enabled,
        profiles_dir,
        runs_examined: profiles.len(),
        targets,
        stale: is_stale(latest_profile_ts, last_up_ts),
        latest_profile_when: format_unix_ts(latest_profile_ts),
        last_up_when,
    }))
}

/// Render the per-run history view (one summary line per profile).
pub fn shell_init_history(ctx: &ExecutionContext, limit: usize) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;
    let profiles = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), limit)?;
    // `read_recent_profiles` already returns newest-first, which is the
    // order users expect for a history listing (most recent at the top
    // of the table). Don't reverse.
    let latest_profile_ts = profiles
        .first()
        .map(|p| parse_unix_ts_from_filename(&p.filename))
        .unwrap_or(0);
    let history = summarize_history(&profiles);
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());

    Ok(ProbeResult::ShellInitHistory(ShellInitHistoryView {
        profiling_enabled,
        profiles_dir,
        rows: history.into_iter().map(into_history_row).collect(),
        stale: is_stale(latest_profile_ts, last_up_ts),
        latest_profile_when: format_unix_ts(latest_profile_ts),
        last_up_when: last_up_ts.map(format_unix_ts).unwrap_or_default(),
    }))
}

fn into_history_row(h: HistoryEntry) -> ShellInitHistoryRow {
    let total_label = h
        .total_us
        .map(humanize_us)
        .unwrap_or_else(|| "unknown".to_string());
    ShellInitHistoryRow {
        filename: h.filename,
        unix_ts: h.unix_ts,
        when: format_unix_ts(h.unix_ts),
        shell: h.shell,
        complete: h.complete,
        total_label,
        user_total_label: humanize_us(h.user_total_us),
        total_us: h.total_us,
        user_total_us: h.user_total_us,
        failed_entries: h.failed_entries,
        entry_count: h.entry_count,
    }
}

/// Format a unix timestamp as `YYYY-MM-DD HH:MM` in UTC. Returns an
/// empty string for `0` (parse-failure sentinel) so the renderer can
/// just print a blank cell.
///
/// Does the calendar math by hand to avoid pulling a dep — chrono is
/// overkill for one display string. Algorithm: Howard Hinnant's
/// civil_from_days.
pub fn format_unix_ts(ts: u64) -> String {
    // 0 is the parse-failure sentinel from `parse_unix_ts_from_filename`;
    // anything past year 9999 is also nonsense in a shell-startup
    // profile (the file format itself is the giveaway). Returning an
    // empty string keeps the renderer predictable even in the face of
    // a tampered-with filename, and bounds the i64 cast on `days`
    // safely below i64::MAX regardless of input.
    const MAX_REASONABLE_TS: u64 = 253_402_300_799; // 9999-12-31T23:59:59 UTC.
    if ts == 0 || ts > MAX_REASONABLE_TS {
        return String::new();
    }
    let secs_per_day: u64 = 86_400;
    let days = (ts / secs_per_day) as i64; // safe: ts < 2.5e11 → days < 3e6
    let secs_of_day = ts % secs_per_day;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{minute:02}")
}

/// Howard Hinnant's `civil_from_days`: convert days since 1970-01-01
/// (UTC) into `(year, month, day)`. Public-domain algorithm.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_unix_ts_handles_zero_and_out_of_range() {
        assert_eq!(format_unix_ts(0), "");
        assert_eq!(format_unix_ts(1_714_000_000), "2024-04-24 23:06");
        // Past year 9999 → empty (defensive ceiling so a tampered
        // filename doesn't produce a nonsense date or risk overflow
        // during the i64 cast on `days`).
        assert_eq!(format_unix_ts(u64::MAX), "");
        assert_eq!(format_unix_ts(253_402_300_800), ""); // 1s past year 9999.
        assert_eq!(format_unix_ts(253_402_300_799), "9999-12-31 23:59");
    }
}
