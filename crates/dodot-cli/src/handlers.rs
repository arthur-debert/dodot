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

/// Post-`up` hook that offers to install plist filters when:
///
/// 1. stdin is a TTY (we're interactive, not in CI/script),
/// 2. at least one pack contains a `*.plist` file,
/// 3. the dodot-plist filter is NOT yet registered in `.git/config`,
/// 4. the user has NOT previously dismissed the prompt.
///
/// All four must hold; otherwise the function returns silently. Errors
/// in any check (e.g. registry corrupt, git missing) go to the dodot
/// log at debug level — visible with `--debug` or in the log file,
/// but silent at default verbosity. This is a nudge, not a critical
/// step, and a noisy stderr line every `up` would be worse UX than
/// silently skipping the offer.
pub fn maybe_prompt_install_filters() {
    if let Err(e) = try_prompt_install_filters() {
        tracing::debug!("plist install-filters prompt skipped: {e}");
    }
}

fn try_prompt_install_filters() -> Result<(), anyhow::Error> {
    use dodot_lib::prompts::PromptRegistry;

    if !crate::interactive::stdin_is_tty() {
        return Ok(());
    }

    let dotfiles_root = discover_dotfiles_root()?;
    let ctx = dodot_lib::packs::orchestration::ExecutionContext::production(&dotfiles_root, false)?;

    let plist_files = commands::git_filters::detect_plist_files(&ctx)?;
    if plist_files.is_empty() {
        return Ok(());
    }
    if commands::git_filters::is_installed(&ctx)? {
        return Ok(());
    }

    let registry_path = ctx.paths.prompts_path();
    let mut registry = PromptRegistry::load(ctx.fs.as_ref(), registry_path)?;
    let prompt_key = "plist.install_filters";
    if registry.is_dismissed(prompt_key) {
        return Ok(());
    }

    // Pick a representative pack name for the prompt body — first
    // file's parent (under dotfiles_root) gives us "<pack>".
    let example_pack = plist_files
        .first()
        .and_then(|p| p.strip_prefix(ctx.paths.dotfiles_root()).ok())
        .and_then(|rel| rel.components().next())
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_else(|| "(unknown)".into());

    let count = plist_files.len();
    let header = format!("dodot detected {count} .plist file(s) in pack `{example_pack}`.");
    let response = crate::interactive::prompt_yes_no_show(&[
        &header,
        "Plist support uses git clean/smudge filters to keep the source diffable.",
        "Install filters now? Run `dodot git-show-filters` to inspect first.",
    ])?;

    match response {
        crate::interactive::YesNoShow::Yes => {
            let r = commands::git_filters::install_filters(&ctx)?;
            eprintln!("{}", r.message);
            for d in &r.details {
                eprintln!("  {d}");
            }
            registry.dismiss(prompt_key);
            registry.save(ctx.fs.as_ref())?;
        }
        crate::interactive::YesNoShow::Show => {
            eprintln!();
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
            eprintln!();
            eprintln!(
                "Run `dodot git-install-filters` to install, or `dodot prompts reset {prompt_key}` to skip and ask later."
            );
            // Don't dismiss — show is informational, the user hasn't
            // committed to either install or skip.
        }
        crate::interactive::YesNoShow::No => {
            // Don't dismiss — re-prompt next up. If the user wants to
            // permanently silence it, they can run
            // `dodot prompts reset <key>` later (well, the inverse —
            // to dismiss it themselves we'd need a `dismiss` CLI verb).
            //
            // For now, "no" means "not now"; a future ergonomics pass
            // can offer "no, and never ask again".
        }
    }
    Ok(())
}

/// Post-`up` hook that offers to install the pre-commit hook when:
///
/// 1. stdin is a TTY (interactive — never prompt under CI / pipes),
/// 2. the dotfiles root is a git working tree,
/// 3. at least one template baseline exists (i.e. the user just
///    deployed one or more templates),
/// 4. the dodot pre-commit block is NOT yet installed,
/// 5. the user has NOT previously dismissed the prompt.
///
/// All five must hold; otherwise the function returns silently.
/// Symmetric in structure with [`maybe_prompt_install_filters`] — same
/// "soft nudge" disposition: failures and skips go to the debug log,
/// not stderr, so a noisy line never appears on routine `up` runs.
pub fn maybe_prompt_install_template_hook() {
    if let Err(e) = try_prompt_install_template_hook() {
        tracing::debug!("template install-hook prompt skipped: {e}");
    }
}

fn try_prompt_install_template_hook() -> Result<(), anyhow::Error> {
    use dodot_lib::prompts::PromptRegistry;

    if !crate::interactive::stdin_is_tty() {
        return Ok(());
    }

    let dotfiles_root = discover_dotfiles_root()?;
    let ctx = dodot_lib::packs::orchestration::ExecutionContext::production(&dotfiles_root, false)?;

    // No templates deployed → nothing to gate on.
    let baselines = dodot_lib::preprocessing::divergence::collect_baselines(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
    )?;
    if baselines.is_empty() {
        return Ok(());
    }

    // Already installed → silent.
    if commands::transform::hook_is_installed(&ctx)? {
        return Ok(());
    }

    // Not a git working tree → installer would error; skip the prompt.
    if !ctx.fs.is_dir(&ctx.paths.dotfiles_root().join(".git")) {
        return Ok(());
    }

    let registry_path = ctx.paths.prompts_path();
    let mut registry = PromptRegistry::load(ctx.fs.as_ref(), registry_path)?;
    let prompt_key = "template.install_hook";
    if registry.is_dismissed(prompt_key) {
        return Ok(());
    }

    let header = format!("dodot deployed {} template file(s).", baselines.len());
    let response = crate::interactive::prompt_yes_no_show(&[
        &header,
        "A pre-commit hook can keep template sources in sync with deployed-side edits",
        "by running `dodot transform check --strict` automatically on every `git commit`.",
        "Install the hook now? Pick `show` to see the block first, or run `dodot transform install-hook` later.",
    ])?;

    match response {
        crate::interactive::YesNoShow::Yes => {
            let r = commands::transform::install_hook(&ctx)?;
            eprintln!("Installed pre-commit hook at {}", r.hook_display_path);
            registry.dismiss(prompt_key);
            registry.save(ctx.fs.as_ref())?;
        }
        crate::interactive::YesNoShow::Show => {
            eprintln!();
            eprintln!("The hook block that would be added to .git/hooks/pre-commit:");
            eprintln!();
            for line in commands::transform::managed_block().lines() {
                eprintln!("    {line}");
            }
            eprintln!();
            eprintln!(
                "Run `dodot transform install-hook` to install, \
                 or `dodot prompts reset {prompt_key}` to skip and ask later."
            );
            // Don't dismiss — show is informational. The user has
            // not committed to install or skip.
        }
        crate::interactive::YesNoShow::No => {
            // Same convention as the plist prompt: "no" means "not
            // now"; we'll re-prompt on the next up.
        }
    }
    Ok(())
}

/// Post-`up` hook that offers to install the template clean filter
/// when:
///
/// 1. stdin is a TTY,
/// 2. dotfiles root is a git working tree,
/// 3. at least one template baseline exists,
/// 4. the dodot pre-commit hook IS installed (we only prompt for
///    the filter after the user already accepted the hook — keeps
///    the install ladder one rung at a time),
/// 5. the dodot-template filter is NOT yet registered in
///    `.git/config`,
/// 6. the user has NOT previously dismissed the prompt.
///
/// All six must hold; otherwise the function returns silently. Same
/// soft-failure pattern as the other post-`up` prompts.
pub fn maybe_prompt_install_template_filter() {
    if let Err(e) = try_prompt_install_template_filter() {
        tracing::debug!("template install-filter prompt skipped: {e}");
    }
}

fn try_prompt_install_template_filter() -> Result<(), anyhow::Error> {
    use dodot_lib::prompts::PromptRegistry;

    if !crate::interactive::stdin_is_tty() {
        return Ok(());
    }

    let dotfiles_root = discover_dotfiles_root()?;
    let ctx = dodot_lib::packs::orchestration::ExecutionContext::production(&dotfiles_root, false)?;

    let baselines = dodot_lib::preprocessing::divergence::collect_baselines(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
    )?;
    if baselines.is_empty() {
        return Ok(());
    }

    // is_installed/install_filter both shell out to `git -C <root>
    // config ...` which handles "not a git repo" semantically (we
    // detect the resulting failure), so we don't need a manual
    // .git-exists probe here. Bonus: this skips the gating problem
    // for worktrees (where .git is a file, not a dir, but `git -C`
    // still works correctly) and submodules.

    if !commands::transform::hook_is_installed(&ctx)? {
        // Wait for the user to install the hook first. Filter is
        // strictly more involved (per-clone setup, .gitattributes
        // edit), so the install ladder asks for the cheaper one
        // first.
        return Ok(());
    }

    if commands::template_install_filter::is_installed(&ctx)? {
        return Ok(());
    }

    let registry_path = ctx.paths.prompts_path();
    let mut registry = PromptRegistry::load(ctx.fs.as_ref(), registry_path)?;
    let prompt_key = "template.install_filter";
    if registry.is_dismissed(prompt_key) {
        return Ok(());
    }

    let response = crate::interactive::prompt_yes_no_show(&[
        "Install the dodot-template git clean filter?",
        "It makes `git status` and `git diff` show deployed-side template edits",
        "between commits, not just at commit time. Adds one block to .git/config",
        "and a single line to .gitattributes.",
    ])?;

    match response {
        crate::interactive::YesNoShow::Yes => {
            let r = commands::template_install_filter::install_filter(&ctx)?;
            eprintln!("{}", r.message);
            for d in &r.details {
                eprintln!("  {d}");
            }
            registry.dismiss(prompt_key);
            registry.save(ctx.fs.as_ref())?;
        }
        crate::interactive::YesNoShow::Show => {
            eprintln!();
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
            eprintln!();
            eprintln!(
                "Run `dodot template install-filter` to install, \
                 or `dodot prompts reset {prompt_key}` to skip and ask later."
            );
        }
        crate::interactive::YesNoShow::No => {
            // Same convention as the other prompts: "no" defers.
        }
    }
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
