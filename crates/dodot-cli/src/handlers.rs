//! CLI handlers — thin wrappers that call dodot-lib commands.
//!
//! Each handler extracts args from clap, builds an ExecutionContext,
//! calls the corresponding dodot-lib function, and returns the result
//! for standout to render.

use std::path::PathBuf;

use standout::cli::{CommandContext, HandlerResult, Output};

use dodot_lib::commands::{self, GroupMode, ViewMode};
use dodot_lib::packs::orchestration::ExecutionContext;

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
