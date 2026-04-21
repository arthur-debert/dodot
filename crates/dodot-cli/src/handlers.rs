//! CLI handlers — thin wrappers that call dodot-lib commands.
//!
//! Each handler extracts args from clap, builds an ExecutionContext,
//! calls the corresponding dodot-lib function, and returns the result
//! for standout to render.

use std::path::PathBuf;

use standout::cli::{CommandContext, HandlerResult, Output};

use dodot_lib::commands;
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

/// Build an ExecutionContext from the current environment.
///
/// Uses `ExecutionContext::production()` with the dotfiles root
/// discovered from env/git. Reads CLI flags when present, defaulting
/// to false for flags not defined on the current subcommand.
fn build_ctx(matches: &clap::ArgMatches) -> Result<ExecutionContext, anyhow::Error> {
    let dotfiles_root = discover_dotfiles_root()?;
    let mut ctx = ExecutionContext::production(&dotfiles_root)?;

    ctx.dry_run = flag_or_false(matches, "dry-run");
    ctx.no_provision = flag_or_false(matches, "no-provision");
    ctx.provision_rerun = flag_or_false(matches, "provision-rerun");
    ctx.force = flag_or_false(matches, "force");

    Ok(ctx)
}

/// Build a read-only context (no dry-run/provision flags).
fn build_readonly_ctx() -> Result<ExecutionContext, anyhow::Error> {
    let dotfiles_root = discover_dotfiles_root()?;
    Ok(ExecutionContext::production(&dotfiles_root)?)
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
    let ctx = build_readonly_ctx()?;
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
    match commands::up::up(filter.as_deref(), &ctx) {
        Ok(result) => {
            print_warnings(&result.warnings);
            Ok(Output::Render(result))
        }
        Err(dodot_lib::DodotError::CrossPackConflict { conflicts }) => {
            // Render conflicts through the same template as status, so the
            // user sees the rich, styled output instead of a raw error dump.
            let home = ctx.paths.home_dir();
            let display_conflicts = conflicts
                .iter()
                .map(|c| commands::DisplayConflict::from_conflict(c, home))
                .collect();
            Ok(Output::Render(commands::PackStatusResult {
                message: None,
                dry_run: ctx.dry_run,
                packs: Vec::new(),
                warnings: Vec::new(),
                conflicts: display_conflicts,
                ignored_packs: Vec::new(),
            }))
        }
        Err(e) => Err(e.into()),
    }
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
    _matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::list::ListResult> {
    let ctx = build_readonly_ctx()?;
    let result = commands::list::list(&ctx)?;
    Ok(Output::Render(result))
}

pub fn init_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::init::InitResult> {
    let ctx = build_readonly_ctx()?;
    let pack_name = matches.get_one::<String>("pack").expect("pack is required");
    let result = commands::init::init(pack_name, &ctx)?;
    Ok(Output::Render(result))
}

pub fn fill_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::fill::FillResult> {
    let ctx = build_readonly_ctx()?;
    let pack_name = matches.get_one::<String>("pack").expect("pack is required");
    let result = commands::fill::fill(pack_name, &ctx)?;
    Ok(Output::Render(result))
}

pub fn adopt_handler(
    matches: &clap::ArgMatches,
    _ctx: &CommandContext,
) -> HandlerResult<commands::PackStatusResult> {
    let ctx = build_readonly_ctx()?;
    let pack_name = matches.get_one::<String>("pack").expect("pack is required");
    let files: Vec<PathBuf> = matches
        .get_many::<String>("files")
        .expect("files is required")
        .map(PathBuf::from)
        .collect();
    let force = matches.get_flag("force");
    let no_follow = matches.get_flag("no-follow");
    let dry_run = matches.get_flag("dry-run");
    let result = commands::adopt::adopt(pack_name, &files, force, no_follow, dry_run, &ctx)
        .map_err(|e| {
            if matches!(e, dodot_lib::DodotError::PackNotFound { .. }) {
                anyhow::anyhow!("{e}\n  Hint: run 'dodot init {pack_name}' first to create it")
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
    let ctx = build_readonly_ctx()?;
    let pack_name = matches.get_one::<String>("pack").expect("pack is required");
    let result = commands::addignore::addignore(pack_name, &ctx)?;
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
    let ctx = ExecutionContext::production(&dotfiles_root)?;
    let script = dodot_lib::shell::generate_init_script(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    print!("{script}");
    Ok(())
}
