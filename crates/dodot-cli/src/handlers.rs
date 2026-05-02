//! CLI handlers — thin wrappers that call dodot-lib commands.
//!
//! Each handler extracts args from clap, builds an ExecutionContext,
//! calls the corresponding dodot-lib function, and returns the result
//! for standout to render.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};

use standout::cli::{CommandContext, HandlerResult, Output};

use dodot_lib::commands::{self, GroupMode, ViewMode};
use dodot_lib::packs::orchestration::ExecutionContext;

/// Side-channel exit code set by handlers that succeeded in producing
/// output but want the process to exit non-zero (e.g.
/// `dodot transform check` when it found divergence). `main.rs` reads
/// this after the dispatch loop and calls `std::process::exit` if it's
/// non-zero. Default 0 — handlers that don't set it have no effect.
///
/// Why a side-channel: standout's `Output` enum only carries
/// Render/Silent/Binary; there's no exit-code variant. Returning `Err`
/// from the handler would let dispatch exit non-zero but would also
/// suppress the normal report rendering. This atomic threads the
/// "succeeded with findings" signal past dispatch without losing the
/// rendered output.
pub(crate) static PENDING_EXIT_CODE: AtomicI32 = AtomicI32::new(0);

/// Read a boolean flag, returning false if the flag is not defined
/// for this subcommand.
fn flag_or_false(matches: &clap::ArgMatches, name: &str) -> bool {
    matches
        .try_get_one::<bool>(name)
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false)
}

/// Resolve the global verbosity flag. `--debug` implies `--verbose`,
/// matching the precedence already used by the logging subsystem.
fn verbose_from(matches: &clap::ArgMatches) -> bool {
    flag_or_false(matches, "verbose") || flag_or_false(matches, "debug")
}

/// Build an ExecutionContext from the current environment.
///
/// Uses `ExecutionContext::production()` with the dotfiles root
/// discovered from env/git. Reads CLI flags when present, defaulting
/// to false for flags not defined on the current subcommand.
fn build_ctx(matches: &clap::ArgMatches) -> Result<ExecutionContext, anyhow::Error> {
    let dotfiles_root = discover_dotfiles_root()?;
    let mut ctx = ExecutionContext::production(&dotfiles_root, verbose_from(matches))?;

    ctx.dry_run = flag_or_false(matches, "dry-run");
    ctx.no_provision = flag_or_false(matches, "no-provision");
    ctx.provision_rerun = flag_or_false(matches, "provision-rerun");
    ctx.force = flag_or_false(matches, "force");
    ctx.view_mode = view_mode_from(matches);
    ctx.group_mode = group_mode_from(matches);

    Ok(ctx)
}

/// Build a read-only context (no dry-run/provision flags).
fn build_readonly_ctx(matches: &clap::ArgMatches) -> Result<ExecutionContext, anyhow::Error> {
    let dotfiles_root = discover_dotfiles_root()?;
    let mut ctx = ExecutionContext::production(&dotfiles_root, verbose_from(matches))?;
    ctx.view_mode = view_mode_from(matches);
    ctx.group_mode = group_mode_from(matches);
    Ok(ctx)
}

fn view_mode_from(matches: &clap::ArgMatches) -> ViewMode {
    if flag_or_false(matches, "short") {
        ViewMode::Short
    } else {
        ViewMode::Full
    }
}

fn group_mode_from(matches: &clap::ArgMatches) -> GroupMode {
    if flag_or_false(matches, "by-status") {
        GroupMode::Status
    } else {
        GroupMode::Name
    }
}

/// Extract pack filter from positional "packs" argument.
fn pack_filter(matches: &clap::ArgMatches) -> Option<Vec<String>> {
    matches
        .get_many::<String>("packs")
        .map(|vals| vals.cloned().collect())
}

/// Discover the dotfiles root directory.
fn discover_dotfiles_root() -> Result<PathBuf, anyhow::Error> {
    // DOTFILES_ROOT env var
    if let Ok(root) = std::env::var("DOTFILES_ROOT") {
        let path = PathBuf::from(root);
        if path.exists() {
            return Ok(path);
        }
    }

    // Git toplevel
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            let toplevel = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !toplevel.is_empty() {
                return Ok(PathBuf::from(toplevel));
            }
        }
    }

    // CWD fallback
    let cwd = std::env::current_dir()?;
    Ok(cwd)
}

// ── Command handlers ────────────────────────────────────────────

pub fn status_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::PackStatusResult> {
    let ctx = build_readonly_ctx(matches)?;
    let filter = pack_filter(matches);
    let result = commands::status::status(filter.as_deref(), &ctx)?;
    print_warnings(&result.warnings);
    Ok(Output::Render(result))
}

pub fn up_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::PackStatusResult> {
    let ctx = build_ctx(matches)?;
    let filter = pack_filter(matches);
    // Use the status-fallback variant so cross-pack conflicts still
    // render the full per-pack listing instead of a bare conflicts dump
    // — `up` and `status` output stay consistent.
    let result = commands::up::up_or_status_for_conflict(filter.as_deref(), &ctx)?;
    print_warnings(&result.warnings);
    Ok(Output::Render(result))
}

pub fn down_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::PackStatusResult> {
    let ctx = build_ctx(matches)?;
    let filter = pack_filter(matches);
    let result = commands::down::down(filter.as_deref(), &ctx)?;
    print_warnings(&result.warnings);
    Ok(Output::Render(result))
}

fn print_warnings(warnings: &[String]) {
    for w in warnings {
        eprintln!("{w}");
    }
}

pub fn list_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::list::ListResult> {
    let ctx = build_readonly_ctx(matches)?;
    let result = commands::list::list(&ctx)?;
    Ok(Output::Render(result))
}

pub fn init_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::init::InitResult> {
    let ctx = build_readonly_ctx(matches)?;
    let pack_name = matches.get_one::<String>("pack").expect("pack is required");
    let result = commands::init::init(pack_name, &ctx)?;
    Ok(Output::Render(result))
}

pub fn fill_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::fill::FillResult> {
    let ctx = build_readonly_ctx(matches)?;
    let pack_name = matches.get_one::<String>("pack").expect("pack is required");
    let result = commands::fill::fill(pack_name, &ctx)?;
    Ok(Output::Render(result))
}

pub fn adopt_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::PackStatusResult> {
    let ctx = build_readonly_ctx(matches)?;
    // `--into` is optional. When absent, adopt infers the pack name
    // from each source's deployed path (XDG layout) or requires the
    // user to supply --into (HOME-direct dotfiles).
    let into = matches.get_one::<String>("into");
    let files: Vec<PathBuf> = matches
        .get_many::<String>("files")
        .expect("files is required")
        .map(PathBuf::from)
        .collect();
    let force = matches.get_flag("force");
    let no_follow = matches.get_flag("no-follow");
    let dry_run = matches.get_flag("dry-run");
    let into_str = into.map(|s| s.as_str());
    let result = commands::adopt::adopt(into_str, &files, force, no_follow, dry_run, &ctx)
        .map_err(|e| {
            if matches!(e, dodot_lib::DodotError::PackNotFound { .. }) {
                let hint_pack = into_str.unwrap_or("<pack>");
                anyhow::anyhow!("{e}\n  Hint: run 'dodot init {hint_pack}' first to create it")
            } else {
                e.into()
            }
        })?;
    print_warnings(&result.warnings);
    Ok(Output::Render(result))
}

pub fn addignore_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::addignore::AddIgnoreResult> {
    let ctx = build_readonly_ctx(matches)?;
    let pack_name = matches.get_one::<String>("pack").expect("pack is required");
    let result = commands::addignore::addignore(pack_name, &ctx)?;
    Ok(Output::Render(result))
}

/// `dodot probe` — bare summary of probe subcommands.
pub fn probe_summary_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::probe::ProbeResult> {
    let ctx = build_readonly_ctx(matches)?;
    Ok(Output::Render(commands::probe::summary(&ctx)?))
}

/// `dodot probe deployment-map` — source↔deployed map view.
pub fn probe_deployment_map_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::probe::ProbeResult> {
    let ctx = build_readonly_ctx(matches)?;
    Ok(Output::Render(commands::probe::deployment_map(&ctx)?))
}

/// `dodot probe show-data-dir [--depth N]` — data-dir tree view.
pub fn probe_show_data_dir_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::probe::ProbeResult> {
    let ctx = build_readonly_ctx(matches)?;
    let depth = matches
        .get_one::<usize>("depth")
        .copied()
        .unwrap_or(commands::probe::DEFAULT_SHOW_DATA_DIR_DEPTH);
    Ok(Output::Render(commands::probe::show_data_dir(&ctx, depth)?))
}

/// `dodot probe app <pack> [--refresh]` — advisory introspection of
/// macOS app-support paths for a pack. See
/// `docs/proposals/macos-paths.lex` §8.4.
pub fn probe_app_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::probe::ProbeResult> {
    let ctx = build_readonly_ctx(matches)?;
    let pack = matches
        .get_one::<String>("pack")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing required argument: pack"))?;
    let refresh = flag_or_false(matches, "refresh");
    Ok(Output::Render(commands::probe::app(&pack, refresh, &ctx)?))
}

/// `dodot transform check [--strict]` — propagate deployed-file edits
/// back to template sources. See `docs/proposals/preprocessing-pipeline.lex`
/// §6 and `docs/proposals/magic.lex`. Exit code 0 = clean, 1 = at
/// least one Patched / Conflict / Missing finding (or, in `--strict`
/// mode, any unresolved dodot-conflict markers in template sources).
///
/// The non-zero exit code is set via [`PENDING_EXIT_CODE`] so the
/// rendered report still prints normally; `main.rs` consults the
/// atomic after dispatch and `std::process::exit`s if it's non-zero.
pub fn transform_check_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::transform::TransformCheckResult> {
    let ctx = build_ctx(matches)?;
    let strict = flag_or_false(matches, "strict");
    let result = commands::transform::check(&ctx, strict)?;
    PENDING_EXIT_CODE.store(result.exit_code(), Ordering::Relaxed);
    Ok(Output::Render(result))
}

/// `dodot transform status` — read-only view of every cached
/// preprocessed file with its current state. Always exits 0.
pub fn transform_status_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::transform::TransformStatusResult> {
    let ctx = build_readonly_ctx(matches)?;
    Ok(Output::Render(commands::transform::status(&ctx)?))
}

/// `dodot git-show-alias [--shell <shell>]` — print the Tier 2
/// shell alias for copy-paste. No filesystem mutation.
pub fn git_show_alias_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::git_alias::ShowAliasResult> {
    let ctx = build_readonly_ctx(matches)?;
    let shell_arg = matches.get_one::<String>("shell").map(String::as_str);
    let shell = commands::git_alias::resolve_shell(shell_arg)?;
    Ok(Output::Render(commands::git_alias::show_alias(
        &ctx, shell,
    )?))
}

/// `dodot git-install-alias [--shell <shell>]` — write the Tier 2
/// alias to the user's shell rc file. Idempotent and additive.
pub fn git_install_alias_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::git_alias::InstallAliasResult> {
    let ctx = build_ctx(matches)?;
    let shell_arg = matches.get_one::<String>("shell").map(String::as_str);
    let shell = commands::git_alias::resolve_shell(shell_arg)?;
    Ok(Output::Render(commands::git_alias::install_alias(
        &ctx, shell,
    )?))
}

/// `dodot template clean --path <path>` — git clean filter
/// passthrough for template sources. Reads stdin (the working-tree
/// source bytes), looks up the matching baseline in the cache,
/// applies the cached reverse-merge if the deployed file has
/// drifted, and writes the result to stdout. Mirrors the
/// `plist clean/smudge` passthrough shape — no standout rendering,
/// just stdin → stdout.
pub fn template_clean_passthrough(matches: &clap::ArgMatches) -> Result<(), anyhow::Error> {
    use dodot_lib::commands::template_clean;
    let dotfiles_root = discover_dotfiles_root()?;
    let ctx = ExecutionContext::production(&dotfiles_root, false)?;

    let path = matches
        .get_one::<String>("path")
        .ok_or_else(|| anyhow::anyhow!("missing required argument: --path"))?;
    let path = std::path::PathBuf::from(path);

    // Path may be relative (git's `%f`) — resolve against the
    // dotfiles root (the working tree git is operating on).
    let path = if path.is_absolute() {
        path
    } else {
        ctx.paths.dotfiles_root().join(path)
    };

    // Resolve [preprocessor.template] no_reverse for the source's
    // pack so the inner clean function can opt out of the slow path
    // for matching files. The first path component below
    // dotfiles_root is the pack name; on any config-loading hiccup
    // we fall back to the empty list (better degraded reverse-merge
    // than a filter that fails the working-tree read).
    let no_reverse_patterns = path
        .strip_prefix(ctx.paths.dotfiles_root())
        .ok()
        .and_then(|rel| rel.components().next())
        .and_then(|c| c.as_os_str().to_str())
        .and_then(|pack| {
            let pack_path = ctx.paths.dotfiles_root().join(pack);
            ctx.config_manager.config_for_pack(&pack_path).ok()
        })
        .map(|cfg| cfg.preprocessor.template.no_reverse)
        .unwrap_or_default();

    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    template_clean::template_clean_stdio(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
        &path,
        &no_reverse_patterns,
        &mut stdin,
        &mut stdout,
    )?;
    Ok(())
}

/// `dodot template install-filter` — register the dodot-template
/// clean filter in `.git/config`. Idempotent.
pub fn template_install_filter_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::template_install_filter::InstallFilterResult> {
    let ctx = build_ctx(matches)?;
    Ok(Output::Render(
        commands::template_install_filter::install_filter(&ctx)?,
    ))
}

/// `dodot refresh [--quiet] [--list-paths]` — copy deployed mtimes
/// onto template sources where they've drifted from the baseline.
/// Used by the Tier 2 shell alias and by the upcoming clean filter
/// flow so `git status` / `git diff` reflect deployed-side edits.
pub fn refresh_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::refresh::RefreshResult> {
    let ctx = build_readonly_ctx(matches)?;
    let mode = if flag_or_false(matches, "list-paths") {
        commands::refresh::RefreshMode::ListPaths
    } else if flag_or_false(matches, "quiet") {
        commands::refresh::RefreshMode::Quiet
    } else {
        commands::refresh::RefreshMode::Report
    };
    Ok(Output::Render(commands::refresh::refresh(&ctx, mode)?))
}

/// `dodot transform install-hook` — write `.git/hooks/pre-commit` with
/// our `dodot transform check --strict` block. Idempotent and additive
/// (preserves any existing hook content). See `commands::transform::
/// install_hook` for behavior detail.
pub fn transform_install_hook_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::transform::InstallHookResult> {
    let ctx = build_ctx(matches)?;
    Ok(Output::Render(commands::transform::install_hook(&ctx)?))
}

/// `dodot probe shell-init` — most recent shell-startup profile.
///
/// Five views, picked by argument shape:
/// - `<pack>[/<file>]` positional: drill-down across recent runs with
///   captured stderr (wins over flags — the user is asking a specific
///   question)
/// - `--errors-only`: cross-history list of failing targets
/// - `--runs N`: per-target percentile aggregate over the last N runs
/// - `--history`: one-row-per-run trend, newest first
/// - default: single-run detail (most recent profile)
pub fn probe_shell_init_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::probe::ProbeResult> {
    let ctx = build_readonly_ctx(matches)?;
    let filter = matches.get_one::<String>("filter").cloned();
    let runs = matches.get_one::<usize>("runs").copied();
    let history = flag_or_false(matches, "history");
    let errors_only = flag_or_false(matches, "errors-only");

    let result = if errors_only {
        commands::probe::shell_init_errors(&ctx, commands::probe::DEFAULT_FILTER_RUNS)?
    } else if let Some(f) = filter {
        commands::probe::shell_init_filter(&ctx, &f, commands::probe::DEFAULT_FILTER_RUNS)?
    } else if let Some(n) = runs {
        commands::probe::shell_init_aggregate(&ctx, n)?
    } else if history {
        commands::probe::shell_init_history(&ctx, commands::probe::DEFAULT_HISTORY_LIMIT)?
    } else {
        commands::probe::shell_init(&ctx)?
    };
    Ok(Output::Render(result))
}

// ── Passthrough handlers (bypass standout rendering) ────────────

/// `dodot config` — delegates to clapfig's config subcommands.
/// Uses `handle_to_string` (clapfig 0.16) for programmatic output.
pub fn config_passthrough(matches: &clap::ArgMatches) -> Result<(), anyhow::Error> {
    let dotfiles_root = discover_dotfiles_root()?;
    let config_cmd = clapfig::ConfigCommand::new();
    let action = config_cmd.parse(matches)?;

    let output = clapfig::Clapfig::builder::<dodot_lib::config::DodotConfig>()
        .app_name("dodot")
        .file_name(".dodot.toml")
        .search_paths(vec![clapfig::SearchPath::Path(dotfiles_root.clone())])
        .search_mode(clapfig::SearchMode::Merge)
        .persist_scope("local", clapfig::SearchPath::Path(dotfiles_root))
        .no_env()
        .handle_to_string(&action)?;

    // Clean up clapfig's Debug-format leak: String("value") → "value"
    let cleaned = clean_debug_format(&output);
    print!("{cleaned}");
    Ok(())
}

/// Remove Rust Debug format wrappers from clapfig output.
/// Replaces `String("value")` with `"value"` in config list output.
fn clean_debug_format(input: &str) -> String {
    let mut result = input.to_string();
    // Iteratively replace String("...") with "..."
    while let Some(start) = result.find("String(\"") {
        let after_prefix = start + 8; // skip 'String("'
        if let Some(end_quote) = result[after_prefix..].find("\")") {
            let value = &result[after_prefix..after_prefix + end_quote];
            let replacement = format!("\"{value}\"");
            result.replace_range(start..after_prefix + end_quote + 2, &replacement);
        } else {
            break;
        }
    }
    result
}

/// `dodot init-sh` — prints shell init script for `eval "$(dodot init-sh)"`.
pub fn init_sh_passthrough() -> Result<(), anyhow::Error> {
    let dotfiles_root = discover_dotfiles_root()?;
    let ctx = ExecutionContext::production(&dotfiles_root, false)?;
    let root_config = ctx.config_manager.root_config()?;
    let script = dodot_lib::shell::generate_init_script(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
        root_config.profiling.enabled,
    )?;
    print!("{script}");
    Ok(())
}

// ── Prompts (registry CLI surface) ─────────────────────────────

pub fn prompts_list_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::prompts::PromptsListResult> {
    let ctx = build_readonly_ctx(matches)?;
    let result = commands::prompts::list(&ctx)?;
    Ok(Output::Render(result))
}

pub fn prompts_reset_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::MessageResult> {
    let ctx = build_readonly_ctx(matches)?;
    let key = matches.get_one::<String>("key").map(String::as_str);
    let all = matches.get_flag("all");
    if all && key.is_some() {
        return Err(anyhow::anyhow!(
            "`dodot prompts reset` accepts a key or --all, not both"
        ));
    }
    if !all && key.is_none() {
        return Err(anyhow::anyhow!(
            "`dodot prompts reset` requires either a key or --all"
        ));
    }
    let result = commands::prompts::reset(key, &ctx)?;
    Ok(Output::Render(result))
}

// ── Git filters ─────────────────────────────────────────────────

pub fn git_install_filters_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::MessageResult> {
    let ctx = build_readonly_ctx(matches)?;
    let result = commands::git_filters::install_filters(&ctx)?;
    Ok(Output::Render(result))
}

pub fn git_show_filters_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::git_filters::ShowFiltersResult> {
    let ctx = build_readonly_ctx(matches)?;
    let result = commands::git_filters::show_filters(&ctx)?;
    Ok(Output::Render(result))
}

/// Post-`up` consolidated installer ladder.
///
/// Replaces three sequential prompts (plist filter, hook, template
/// filter) with a single Y/n covering whichever rungs apply. Spec:
/// `docs/proposals/magic.lex` §"What This Costs the User" promises
/// "one Y/n to install the clean/smudge filters and the pre-commit
/// hook"; this function is the implementation.
///
/// Behavior:
/// - **Yes** installs every applicable rung whose component dismissal
///   is not set, in dependency order (hook first; template filter
///   only after hook installs cleanly). Each successful install
///   dismisses its component key.
/// - **Show** walks each rung individually, printing the previewable
///   block (config, hook script, .gitattributes lines) without
///   touching the registry. The user can read, then re-run `up` to
///   answer the prompt for real.
/// - **No** dismisses every applicable component key so the ladder
///   never re-prompts on subsequent `up` runs. The user can resurface
///   any rung individually with `dodot prompts reset <key>`.
///
/// Tier-3 alias (`dodot git-install-alias`) is intentionally not in
/// the ladder per the magic.lex spec — it ships separately.
///
/// Soft-fail like every other post-`up` prompt: errors and skips go
/// to the debug log so a noisy stderr line never appears on routine
/// runs.
pub fn maybe_prompt_install_ladder() {
    if let Err(e) = try_prompt_install_ladder() {
        tracing::debug!("install-ladder prompt skipped: {e}");
    }
}

/// Per-rung gating decision. Each rung answers two questions: is the
/// rung *applicable* (does the user's repo state make it relevant?),
/// and is its component dismissal *outstanding* (the user hasn't
/// previously chosen `no`). A rung shows up in the ladder only when
/// both are yes; "yes" installs it, "no" dismisses it.
struct LadderRung {
    /// Display label for the prompt body and the show output.
    name: &'static str,
    /// Catalog key for this rung's component-level dismissal.
    component_key: &'static str,
    /// One-liner the prompt body uses to enumerate what would be
    /// installed.
    summary: &'static str,
}

const RUNG_HOOK: LadderRung = LadderRung {
    name: "pre-commit hook",
    component_key: "template.install_hook",
    summary: "pre-commit hook (refreshes templates + safety check on `git commit`)",
};

const RUNG_PLIST_FILTER: LadderRung = LadderRung {
    name: "plist clean/smudge filters",
    component_key: "plist.install_filters",
    summary: "dodot-plist clean/smudge filters (binary↔XML for plists)",
};

const RUNG_TEMPLATE_FILTER: LadderRung = LadderRung {
    name: "template clean filter",
    component_key: "template.install_filter",
    summary: "dodot-template clean filter (live diffs in `git status` / `git diff`)",
};

fn try_prompt_install_ladder() -> Result<(), anyhow::Error> {
    use dodot_lib::prompts::PromptRegistry;

    if !crate::interactive::stdin_is_tty() {
        return Ok(());
    }

    let dotfiles_root = discover_dotfiles_root()?;
    let ctx = dodot_lib::packs::orchestration::ExecutionContext::production(&dotfiles_root, false)?;

    // Determine applicability per rung. Each rung is considered iff
    // the user has state that justifies it AND that state isn't
    // already wired up.
    let templates_present = !dodot_lib::preprocessing::divergence::collect_baselines(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
    )?
    .is_empty();
    let plist_present = !commands::git_filters::detect_plist_files(&ctx)?.is_empty();
    let in_git_worktree = ctx.fs.is_dir(&ctx.paths.dotfiles_root().join(".git"));

    let hook_applicable =
        templates_present && in_git_worktree && !commands::transform::hook_is_installed(&ctx)?;
    let plist_filter_applicable = plist_present && !commands::git_filters::is_installed(&ctx)?;
    let template_filter_applicable =
        templates_present && !commands::template_install_filter::is_installed(&ctx)?;

    let registry_path = ctx.paths.prompts_path();
    let mut registry = PromptRegistry::load(ctx.fs.as_ref(), registry_path)?;
    let ladder_key = "magic.install_ladder";
    if registry.is_dismissed(ladder_key) {
        return Ok(());
    }

    // A rung enters the ladder only when applicable AND its
    // component dismissal isn't set. The component check lets the
    // user opt out of a single rung (via `dodot prompts <key>`) while
    // still hearing about the others.
    let mut active_rungs: Vec<&LadderRung> = Vec::new();
    if hook_applicable && !registry.is_dismissed(RUNG_HOOK.component_key) {
        active_rungs.push(&RUNG_HOOK);
    }
    if plist_filter_applicable && !registry.is_dismissed(RUNG_PLIST_FILTER.component_key) {
        active_rungs.push(&RUNG_PLIST_FILTER);
    }
    if template_filter_applicable && !registry.is_dismissed(RUNG_TEMPLATE_FILTER.component_key) {
        active_rungs.push(&RUNG_TEMPLATE_FILTER);
    }
    if active_rungs.is_empty() {
        return Ok(());
    }

    // Build the prompt body. Header summarises detection; bullet list
    // enumerates what would be installed; closing line frames the
    // ask.
    let mut body: Vec<String> = Vec::new();
    body.push("dodot can wire git up to handle the files in this repo:".into());
    for rung in &active_rungs {
        body.push(format!("  - {}", rung.summary));
    }
    body.push(String::new());
    if active_rungs.len() == 1 {
        body.push("Install it now? `show` previews first; `no` skips and won't ask again.".into());
    } else {
        body.push(format!(
            "Install all {}? `show` previews each; `no` skips and won't ask again.",
            active_rungs.len()
        ));
    }
    let body_refs: Vec<&str> = body.iter().map(String::as_str).collect();
    let response = crate::interactive::prompt_yes_no_show(&body_refs)?;

    match response {
        crate::interactive::YesNoShow::Yes => {
            install_ladder_yes(&ctx, &active_rungs, &mut registry)?;
            registry.save(ctx.fs.as_ref())?;
        }
        crate::interactive::YesNoShow::Show => {
            install_ladder_show(&ctx, &active_rungs)?;
            // Show is informational — no dismissal. Re-running `up`
            // will offer the same prompt.
        }
        crate::interactive::YesNoShow::No => {
            // Per #112: "no" dismisses every component key so
            // subsequent `up` runs don't re-prompt. The umbrella
            // `magic.install_ladder` key is also dismissed so a
            // future rung becoming applicable doesn't unilaterally
            // re-open the ladder against the user's wishes.
            for rung in &active_rungs {
                registry.dismiss(rung.component_key);
            }
            registry.dismiss(ladder_key);
            registry.save(ctx.fs.as_ref())?;
            eprintln!(
                "Skipped install ladder. Re-run `dodot prompts reset <key>` to surface a \
                 specific rung again, or `dodot prompts reset {ladder_key}` for the lot."
            );
        }
    }
    Ok(())
}

fn install_ladder_yes(
    ctx: &dodot_lib::packs::orchestration::ExecutionContext,
    rungs: &[&LadderRung],
    registry: &mut dodot_lib::prompts::PromptRegistry,
) -> Result<(), anyhow::Error> {
    // Hook installs first — the template filter rung is only useful
    // when the hook is also installed (the safety gate against
    // committing unresolved markers lives in the hook). If the user
    // accepted both, the hook lands first so the filter's value is
    // realized immediately.
    let install_hook = rungs
        .iter()
        .any(|r| r.component_key == RUNG_HOOK.component_key);
    let install_plist = rungs
        .iter()
        .any(|r| r.component_key == RUNG_PLIST_FILTER.component_key);
    let install_template_filter = rungs
        .iter()
        .any(|r| r.component_key == RUNG_TEMPLATE_FILTER.component_key);

    if install_hook {
        let r = commands::transform::install_hook(ctx)?;
        eprintln!("Installed pre-commit hook at {}", r.hook_display_path);
        registry.dismiss(RUNG_HOOK.component_key);
    }
    if install_plist {
        let r = commands::git_filters::install_filters(ctx)?;
        eprintln!("{}", r.message);
        for d in &r.details {
            eprintln!("  {d}");
        }
        registry.dismiss(RUNG_PLIST_FILTER.component_key);
    }
    if install_template_filter {
        let r = commands::template_install_filter::install_filter(ctx)?;
        eprintln!("{}", r.message);
        for d in &r.details {
            eprintln!("  {d}");
        }
        registry.dismiss(RUNG_TEMPLATE_FILTER.component_key);
    }
    Ok(())
}

fn install_ladder_show(
    ctx: &dodot_lib::packs::orchestration::ExecutionContext,
    rungs: &[&LadderRung],
) -> Result<(), anyhow::Error> {
    for rung in rungs {
        eprintln!();
        eprintln!("── {} ──", rung.name);
        match rung.component_key {
            "template.install_hook" => {
                eprintln!("Block to be added to .git/hooks/pre-commit:");
                eprintln!();
                for line in commands::transform::managed_block().lines() {
                    eprintln!("    {line}");
                }
            }
            "plist.install_filters" => {
                eprintln!("Add to .git/config (per clone, per machine):");
                eprintln!();
                for line in commands::git_filters::config_block_text().lines() {
                    eprintln!("    {line}");
                }
                eprintln!();
                eprintln!("Add to .gitattributes (committed in the repo):");
                let raw = ctx.config_manager.root_config()?.symlink.plist_extensions;
                let extensions = commands::git_filters::normalize_plist_extensions(&raw);
                for line in commands::git_filters::gitattributes_lines(&extensions) {
                    eprintln!("    {line}");
                }
            }
            "template.install_filter" => {
                eprintln!("Add to .git/config (per clone, per machine):");
                eprintln!();
                for line in commands::template_install_filter::config_block_text().lines() {
                    eprintln!("    {line}");
                }
                eprintln!();
                eprintln!("Add to .gitattributes (committed in the repo):");
                eprintln!(
                    "    {}",
                    commands::template_install_filter::gitattributes_line()
                );
            }
            _ => unreachable!("unknown rung key"),
        }
    }
    eprintln!();
    eprintln!("Re-run `dodot up` to answer the prompt, or run the install commands directly.");
    Ok(())
}

/// Post-`up` cfprefsd cache-invalidation prompt (macOS only).
///
/// Fires when the previous `dodot up` (or this one's deploy phase)
/// dropped the `cfprefsd-needs-invalidation` marker — i.e. some
/// plist file in an active pack has changed since the prior
/// successful `up`. macOS's `cfprefsd` aggressively caches plist
/// values; running apps may continue reading the stale value until
/// `cfprefsd` is restarted. `killall cfprefsd` clears the cache and
/// cfprefsd respawns immediately. See `docs/proposals/plists.lex`
/// §6.4 and issue #109.
///
/// On every outcome (yes / no / show), the marker is cleared so the
/// prompt doesn't re-fire on the next `up` unless drift recurs. "No"
/// additionally dismisses the catalog entry so the prompt never
/// returns until the user runs `dodot prompts reset`.
pub fn maybe_prompt_invalidate_cfprefsd() {
    if let Err(e) = try_prompt_invalidate_cfprefsd() {
        tracing::debug!("cfprefsd-invalidate prompt skipped: {e}");
    }
}

fn try_prompt_invalidate_cfprefsd() -> Result<(), anyhow::Error> {
    use dodot_lib::prompts::PromptRegistry;

    // cfprefsd is macOS-only.
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    if !crate::interactive::stdin_is_tty() {
        return Ok(());
    }

    let dotfiles_root = discover_dotfiles_root()?;
    let ctx = dodot_lib::packs::orchestration::ExecutionContext::production(&dotfiles_root, false)?;

    if !dodot_lib::probe::cfprefsd_marker_exists(ctx.fs.as_ref(), ctx.paths.as_ref()) {
        return Ok(());
    }

    let registry_path = ctx.paths.prompts_path();
    let mut registry = PromptRegistry::load(ctx.fs.as_ref(), registry_path)?;
    let prompt_key = "plist.cfprefsd_invalidate";
    if registry.is_dismissed(prompt_key) {
        // User previously chose "no". Clear the marker so it doesn't
        // accumulate on subsequent ups; respect the dismissal.
        dodot_lib::probe::clear_cfprefsd_marker(ctx.fs.as_ref(), ctx.paths.as_ref());
        return Ok(());
    }

    let response = crate::interactive::prompt_yes_no_show(&[
        "dodot detected a plist change on this `up`.",
        "macOS caches plist values via cfprefsd; running apps may show stale values until",
        "cfprefsd is restarted. Run `killall cfprefsd` now? (cfprefsd respawns immediately;",
        "no data loss.) `no` skips and won't ask again.",
    ])?;

    match response {
        crate::interactive::YesNoShow::Yes => {
            // Use the same CommandRunner the rest of dodot uses so
            // tests can stub it. `killall` returns 1 if no matching
            // process — that's fine, treat any exit as success for
            // prompt purposes (the cache is cleared either way: an
            // empty cache is the desired end state).
            let runner = ctx.command_runner.as_ref();
            let result = runner.run("killall", &["cfprefsd".into()]);
            match result {
                Ok(_) => eprintln!("Ran `killall cfprefsd`."),
                Err(e) => eprintln!(
                    "Tried to run `killall cfprefsd` but failed: {e}. \
                     You can run it yourself if running apps still show stale plist values."
                ),
            }
            registry.dismiss(prompt_key);
            registry.save(ctx.fs.as_ref())?;
        }
        crate::interactive::YesNoShow::No => {
            registry.dismiss(prompt_key);
            registry.save(ctx.fs.as_ref())?;
            eprintln!(
                "Skipped. Re-run `dodot prompts reset {prompt_key}` to surface this prompt again."
            );
        }
        crate::interactive::YesNoShow::Show => {
            eprintln!();
            eprintln!("Would run:");
            eprintln!("    killall cfprefsd");
            eprintln!();
            eprintln!("That tells macOS to drop its cached plist values; the daemon respawns");
            eprintln!(
                "automatically. No data loss; running apps re-read the plist on next access."
            );
            // Show doesn't clear or dismiss — re-running `up`
            // re-prompts.
            return Ok(());
        }
    }

    // Yes/No: marker handled, clear it.
    dodot_lib::probe::clear_cfprefsd_marker(ctx.fs.as_ref(), ctx.paths.as_ref());
    Ok(())
}

/// `dodot plist clean` — read any plist on stdin, write canonical XML on stdout.
///
/// Used as a git clean filter: `git add` invokes this on the working-tree
/// binary and stores the resulting XML in the index.
pub fn plist_clean_passthrough() -> Result<(), anyhow::Error> {
    use std::io::{Read, Write};
    let mut buf = Vec::new();
    std::io::stdin().lock().read_to_end(&mut buf)?;
    let out = dodot_lib::plists::clean(&buf)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&out)?;
    stdout.flush()?;
    Ok(())
}

/// `dodot plist smudge` — read XML on stdin, write binary plist on stdout.
///
/// Used as a git smudge filter: `git checkout` invokes this on the indexed
/// XML and writes the resulting binary into the working tree.
pub fn plist_smudge_passthrough() -> Result<(), anyhow::Error> {
    use std::io::{Read, Write};
    let mut buf = Vec::new();
    std::io::stdin().lock().read_to_end(&mut buf)?;
    let out = dodot_lib::plists::smudge(&buf)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&out)?;
    stdout.flush()?;
    Ok(())
}
