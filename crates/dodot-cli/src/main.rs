use clap::{Arg, ArgAction, Command as ClapCommand};
use standout::cli::{App, CommandGroup};
use standout::{EmbeddedTemplates, OutputMode};

use dodot_lib::render;

mod handlers;
mod help;
mod interactive;
mod logging;
mod safety;
#[cfg(test)]
mod safety_tests;
mod tutorial;

fn main() {
    // Intercept --help / -h / `help [...]` ourselves before standout's
    // help dispatch runs. Each command has hand-written help text in
    // src/help/<cmd>.txt — see the `help` module docstring for why we
    // own the dispatch instead of plumbing through standout's data
    // extractor.
    //
    // argv is read as `OsString`, never `String`: `roots forget` accepts
    // a native non-Unicode path (its Clap parser is `OsString` for
    // exactly that), and `std::env::args()` panics on any such argument
    // before either the pre-scan or Clap could see it. The same applies
    // to parsing below — `parse_with` iterates `std::env::args()`
    // internally, so the collected `OsString` argv is handed to
    // `parse_from` instead.
    let raw_args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if let Some(path) = help::detect_help_request(&raw_args) {
        let text = help::lookup(&path);
        // Help text is for humans — render with Auto so it picks up
        // ANSI styling on a TTY and stays plain on pipes.
        print!("{}", help::render(text, OutputMode::Auto));
        return;
    }

    let app = build_app();

    // parse_from handles help rendering (with command groups) and exits if help requested
    let matches = app.parse_from(build_clap_command(), raw_args);

    // Passthrough: plist clean/smudge (git filter binary stdin/stdout).
    // Dispatched BEFORE logging::init so git filters — which fire on
    // every `git status` / `git diff` / `git add` — don't pay for
    // log-dir creation, rotation, and the file-handle guard on every
    // invocation. These filters also must bypass standout entirely —
    // smudge emits binary plist bytes, and any extra logging on stdout
    // would corrupt the filter output git stages or checks out.
    if let Some(("plist", sub)) = matches.subcommand() {
        let result = match sub.subcommand_name() {
            Some("clean") => handlers::plist_clean_passthrough(),
            Some("smudge") => handlers::plist_smudge_passthrough(),
            _ => {
                let _ = build_clap_command()
                    .find_subcommand("plist")
                    .cloned()
                    .map(|mut c| c.print_help());
                println!();
                return;
            }
        };
        if let Err(e) = result {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }

    // Passthrough: `template clean` (git clean filter — stdin/stdout
    // bytes, must bypass standout for the same reasons as plist's
    // filter passthroughs). `template install-filter` is NOT
    // passthrough — it goes through standout dispatch normally.
    if let Some(("template", sub)) = matches.subcommand() {
        if let Some(("clean", clean_matches)) = sub.subcommand() {
            if let Err(e) = handlers::template_clean_passthrough(clean_matches) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
            return;
        }
    }

    let verbosity = if matches.get_flag("debug") {
        logging::Verbosity::Debug
    } else if matches.get_flag("verbose") {
        logging::Verbosity::Verbose
    } else {
        logging::Verbosity::Quiet
    };
    let log_dir = dodot_lib::paths::XdgPather::log_dir_from_env();
    let _log_guard = logging::init(&log_dir, verbosity);

    // Passthrough: config (clapfig handles its own output)
    if let Some(("config", sub_matches)) = matches.subcommand() {
        // With no config subcommand, show config help instead of
        // falling through to config list.
        if sub_matches.subcommand().is_none() {
            let cmd = build_clap_command();
            let config_cmd = cmd
                .get_subcommands()
                .find(|c| c.get_name() == "config")
                .cloned();
            if let Some(mut c) = config_cmd {
                let _ = c.print_help();
                println!();
            }
            return;
        }
        if let Err(e) = handlers::config_passthrough(sub_matches) {
            exit_passthrough_failure(e);
        }
        return;
    }

    // Passthrough: init-sh (raw stdout for shell eval)
    if matches.subcommand_matches("init-sh").is_some() {
        if let Err(e) = handlers::init_sh_passthrough() {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }

    // Passthrough: tutorial (interactive — multiple prompts and outputs,
    // doesn't fit standout's one-shot render-and-print dispatch).
    if let Some(("tutorial", sub)) = matches.subcommand() {
        let opts = tutorial::Options {
            reset: sub.get_flag("reset"),
            from: sub.get_one::<String>("from").cloned(),
            mode: standout::OutputMode::Auto,
        };
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        if let Err(e) = tutorial::run(opts, &mut handle) {
            exit_passthrough_failure(e);
        }
        return;
    }

    // All other commands go through standout dispatch.
    // Capture the matched subcommand name now so the post-dispatch hook
    // (which runs after standout consumed `matches`) can know what ran.
    let subcommand = matches.subcommand_name().map(str::to_string);
    let output_mode = app.extract_output_mode(&matches);
    match app.dispatch(matches, output_mode) {
        standout::cli::RunResult::Handled(output) => {
            println!("{output}");
            // Post-up nudges. Both fire only after a successful `up`
            // and are soft (failures land in the debug log, never
            // stderr); each handler's docstring covers what it offers
            // and when it applies. `up --dry-run` lands here too, but
            // its gate authorized nothing, so `authorized_root` inside
            // each handler turns both into a skip (ADR-0002).
            if subcommand.as_deref() == Some("up") {
                handlers::maybe_prompt_install_ladder();
                handlers::maybe_prompt_invalidate_cfprefsd();
            }
            // `dodot transform check` may have set a non-zero exit code
            // via PENDING_EXIT_CODE; the pre-commit hook counts on the
            // process exiting 1. Read after print so the user still
            // sees the rendered report when we're about to exit.
            let pending = handlers::PENDING_EXIT_CODE.load(std::sync::atomic::Ordering::Relaxed);
            if pending != 0 {
                std::process::exit(pending);
            }
        }
        standout::cli::RunResult::Silent => {}
        standout::cli::RunResult::NoMatch(_) => {
            let _ = build_clap_command().print_help();
            println!();
        }
        standout::cli::RunResult::Binary(data, content_type) => {
            eprintln!(
                "error: unexpected binary output ({content_type}, {} bytes)",
                data.len()
            );
            std::process::exit(1);
        }
        // Handler / hook / output-write errors. standout's
        // `RunResult::Error` carries the formatted error string; print
        // it to stderr and exit non-zero so scripts piping with `&&`
        // and CI invocations see the failure. Needs standout >= 7.6.2 —
        // earlier versions reported these as `Handled` and exited 0.
        //
        // The status comes from the failure rather than being hard-coded,
        // because Safety Lock's gate declares its own: a refused root exits
        // 1 with the diagnostic exactly as the gate wrote it, and stdout
        // untouched (`safety::refusal`).
        standout::cli::RunResult::Error(err) => {
            eprintln!("{err}");
            std::process::exit(err.exit_status().code().into());
        }
        // `RunResult` is `#[non_exhaustive]` cross-crate; the wildcard
        // keeps dodot building if a future variant is added without
        // surprising callers. Treat unknown variants as a soft error.
        _ => {
            eprintln!("error: unknown dispatch result variant");
            std::process::exit(1);
        }
    }
}

/// Report a passthrough route's failure on stderr and exit 1.
///
/// A Safety Lock [`safety::Refusal`] is printed exactly as the gate wrote it
/// — its diagnostic is the whole message, the same shape a refusal has under
/// Standout dispatch — while any other error keeps the conventional
/// `error:` prefix. The whole error chain is scanned, not just the outermost
/// error, so a `.context(...)` added somewhere along the bubble-up cannot
/// silently demote the refusal to a prefixed error.
fn exit_passthrough_failure(e: anyhow::Error) -> ! {
    match e
        .chain()
        .find_map(|cause| cause.downcast_ref::<safety::Refusal>())
    {
        Some(refusal) => eprintln!("{refusal}"),
        None => eprintln!("error: {e}"),
    }
    std::process::exit(1);
}

/// Templates shared with `dodot-lib` (via its `render` module). The
/// CLI ships no private templates of its own, so we build the embedded
/// source directly from the `pub const` strings exported by the lib
/// rather than scanning a separate `src/templates` directory — this
/// keeps the lib self-contained (no cross-crate `include_str!`) while
/// still giving standout the same `EmbeddedTemplates` handle.
static TEMPLATE_ENTRIES: &[(&str, &str)] = &[
    ("pack-status.jinja", render::TEMPLATE_PACK_STATUS),
    ("list.jinja", render::TEMPLATE_LIST),
    ("message.jinja", render::TEMPLATE_MESSAGE),
    ("probe.jinja", render::TEMPLATE_PROBE),
    ("git-filters.jinja", render::TEMPLATE_GIT_FILTERS),
    ("prompts-list.jinja", render::TEMPLATE_PROMPTS_LIST),
    ("roots-list.jinja", render::TEMPLATE_ROOTS_LIST),
    ("transform-check.jinja", render::TEMPLATE_TRANSFORM_CHECK),
    (
        "transform-install-hook.jinja",
        render::TEMPLATE_TRANSFORM_INSTALL_HOOK,
    ),
    ("refresh.jinja", render::TEMPLATE_REFRESH),
    (
        "template-install-filter.jinja",
        render::TEMPLATE_TEMPLATE_INSTALL_FILTER,
    ),
    ("transform-status.jinja", render::TEMPLATE_TRANSFORM_STATUS),
    ("git-show-alias.jinja", render::TEMPLATE_GIT_SHOW_ALIAS),
    (
        "git-install-alias.jinja",
        render::TEMPLATE_GIT_INSTALL_ALIAS,
    ),
    ("install.jinja", render::TEMPLATE_INSTALL),
    ("secret-probe.jinja", render::TEMPLATE_SECRET_PROBE),
    ("secret-list.jinja", render::TEMPLATE_SECRET_LIST),
];

fn pack_status_width(terminal_width: Option<usize>) -> usize {
    terminal_width.unwrap_or(80)
}

fn build_app() -> App {
    let mut builder = App::builder()
        .help_handling(true)
        .templates(EmbeddedTemplates::new(TEMPLATE_ENTRIES, ""))
        .styles(standout::embed_styles!("src/styles"))
        .default_theme("dodot")
        // Human pack-status rows fill the detected terminal width. Pipes and
        // other width-less render targets use the same stable 80-column
        // fallback as the library renderer and fixed-width tests.
        .context_fn(
            "terminal_width",
            |ctx: &standout::context::RenderContext<'_>| {
                pack_status_width(ctx.terminal_width).into()
            },
        )
        .command("status", handlers::status_handler, "pack-status")
        .expect("register status")
        .command("up", handlers::up_handler, "pack-status")
        .expect("register up")
        .command("down", handlers::down_handler, "pack-status")
        .expect("register down")
        .command("list", handlers::list_handler, "list")
        .expect("register list")
        .command("init", handlers::init_handler, "message")
        .expect("register init")
        .command("fill", handlers::fill_handler, "message")
        .expect("register fill")
        .command("adopt", handlers::adopt_handler, "pack-status")
        .expect("register adopt")
        .command("addignore", handlers::addignore_handler, "message")
        .expect("register addignore")
        .command("probe", handlers::probe_summary_handler, "probe")
        .expect("register probe")
        .command(
            "probe.deployment-map",
            handlers::probe_deployment_map_handler,
            "probe",
        )
        .expect("register probe.deployment-map")
        .command(
            "probe.show-data-dir",
            handlers::probe_show_data_dir_handler,
            "probe",
        )
        .expect("register probe.show-data-dir")
        .command(
            "probe.shell-init",
            handlers::probe_shell_init_handler,
            "probe",
        )
        .expect("register probe.shell-init")
        .command("probe.app", handlers::probe_app_handler, "probe")
        .expect("register probe.app")
        .command(
            "git-install-filters",
            handlers::git_install_filters_handler,
            "message",
        )
        .expect("register git-install-filters")
        .command(
            "git-show-filters",
            handlers::git_show_filters_handler,
            "git-filters",
        )
        .expect("register git-show-filters")
        .command(
            "prompts.list",
            handlers::prompts_list_handler,
            "prompts-list",
        )
        .expect("register prompts.list")
        .command("prompts.reset", handlers::prompts_reset_handler, "message")
        .expect("register prompts.reset")
        .command(
            "transform.check",
            handlers::transform_check_handler,
            "transform-check",
        )
        .expect("register transform.check")
        .command(
            "transform.install-hook",
            handlers::transform_install_hook_handler,
            "transform-install-hook",
        )
        .expect("register transform.install-hook")
        .command("refresh", handlers::refresh_handler, "refresh")
        .expect("register refresh")
        .command("reset", handlers::reset_handler, "message")
        .expect("register reset")
        .command(
            "template.install-filter",
            handlers::template_install_filter_handler,
            "template-install-filter",
        )
        .expect("register template.install-filter")
        .command(
            "transform.status",
            handlers::transform_status_handler,
            "transform-status",
        )
        .expect("register transform.status")
        .command(
            "git-show-alias",
            handlers::git_show_alias_handler,
            "git-show-alias",
        )
        .expect("register git-show-alias")
        .command(
            "git-install-alias",
            handlers::git_install_alias_handler,
            "git-install-alias",
        )
        .expect("register git-install-alias")
        .command("install", handlers::install_handler, "install")
        .expect("register install")
        .command(
            "secret.probe",
            handlers::secret_probe_handler,
            "secret-probe",
        )
        .expect("register secret.probe")
        .command("secret.list", handlers::secret_list_handler, "secret-list")
        .expect("register secret.list")
        .command("roots.list", handlers::roots_list_handler, "roots-list")
        .expect("register roots.list")
        .command("roots.forget", handlers::roots_forget_handler, "message")
        .expect("register roots.forget")
        .command_groups(vec![
            CommandGroup {
                title: "Core".into(),
                help: None,
                commands: vec![
                    Some("up".into()),
                    Some("down".into()),
                    Some("status".into()),
                    Some("list".into()),
                ],
            },
            CommandGroup {
                title: "Helpers".into(),
                help: None,
                commands: vec![
                    Some("adopt".into()),
                    Some("init".into()),
                    Some("fill".into()),
                    Some("addignore".into()),
                ],
            },
            CommandGroup {
                title: "Diagnostics".into(),
                help: None,
                commands: vec![Some("probe".into())],
            },
            CommandGroup {
                title: "Git filters".into(),
                help: None,
                commands: vec![
                    Some("git-install-filters".into()),
                    Some("git-show-filters".into()),
                    Some("git-show-alias".into()),
                    Some("git-install-alias".into()),
                    Some("plist".into()),
                    Some("template".into()),
                ],
            },
            CommandGroup {
                title: "Misc".into(),
                help: None,
                commands: vec![
                    Some("roots".into()),
                    Some("install".into()),
                    Some("transform".into()),
                    Some("refresh".into()),
                    Some("reset".into()),
                    Some("tutorial".into()),
                    Some("init-sh".into()),
                    Some("prompts".into()),
                    Some("config".into()),
                    Some("help".into()),
                ],
            },
        ]);

    // Safety Lock's gate, wired to every dispatched command. Standout looks
    // hooks up by exact command path and offers no all-commands seam, so the
    // one hook implementation is registered once per row of the taxonomy —
    // and a command absent from that table gets no root at all, which is what
    // makes an unclassified new command fail loudly instead of quietly
    // escaping the gate (`safety::COMMAND_SENSITIVITY`).
    for (path, sensitivity) in safety::COMMAND_SENSITIVITY {
        builder = builder.hooks(path, safety::hook(*sensitivity));
    }

    let app = builder.build().expect("app build");

    // The two halves of the taxonomy must stay disjoint: a passthrough row
    // for a path that is also hooked would be dead policy pretending to be
    // the live one. The matrix test walks the whole command tree; this
    // registration-time check catches the overlap even in a build whose
    // tests were not run.
    if cfg!(debug_assertions) {
        for (path, _) in safety::PASSTHROUGH_POLICY {
            assert!(
                app.get_hooks(path).is_none(),
                "`{path}` is declared in both COMMAND_SENSITIVITY and PASSTHROUGH_POLICY"
            );
        }
    }

    app
}

fn build_clap_command() -> ClapCommand {
    // `handlers::config_command` is the single source of truth for the
    // configured `ConfigCommand` — both this registration site and the
    // `config_passthrough` parser route through it. See its docstring
    // for why `--output` is renamed to `--out`.
    let config_cmd = handlers::config_command();

    ClapCommand::new("dodot")
        .about("A dotfiles manager that uses symlinks for live editing")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .help("Enable verbose logging to stderr")
                .global(true)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("debug")
                .long("debug")
                .help("Enable debug logging to stderr (implies --verbose)")
                .global(true)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("short")
                .long("short")
                .help("Collapse each pack to one summary line")
                .global(true)
                .conflicts_with("full")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("full")
                .long("full")
                .help("Show every file per pack (default)")
                .global(true)
                .conflicts_with("short")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("by-status")
                .long("by-status")
                .help("Group packs by aggregated status (deployed / pending / error)")
                .global(true)
                .conflicts_with("by-name")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("by-name")
                .long("by-name")
                .help("List packs in discovery order (default)")
                .global(true)
                .conflicts_with("by-status")
                .action(ArgAction::SetTrue),
        )
        .subcommand(
            ClapCommand::new("status")
                .about("Show deployment status of packs")
                .after_help("Icons: ➞ symlink  ⚙ shell/homebrew  + path  × install script")
                .arg(
                    Arg::new("packs")
                        .help("Pack names to show (all if omitted)")
                        .num_args(0..)
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("check-drift")
                        .long("check-drift")
                        .help("Hash deployed externals and report any divergence from the configured signature (opt-in; can be slow for big trees)")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("diff")
                        .long("diff")
                        .help("For run-once files reporting `older version`, show the unified diff between the previously-run snapshot and the current source")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            ClapCommand::new("up")
                .about("Deploy packs")
                .arg(
                    Arg::new("packs")
                        .help("Pack names to deploy (all if omitted)")
                        .num_args(0..)
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .help("Show what would be done without making changes")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("no-provision")
                        .long("no-provision")
                        .help("Skip install scripts and Brewfile")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("provision-rerun")
                        .long("provision-rerun")
                        .help("Force re-run of install scripts")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .help("Overwrite pre-existing files at target locations")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            ClapCommand::new("down")
                .about("Remove deployed state for packs")
                .arg(
                    Arg::new("packs")
                        .help("Pack names to deactivate (all if omitted)")
                        .num_args(0..)
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .help("Show what would be done without making changes")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(ClapCommand::new("list").about("List all packs"))
        .subcommand(
            ClapCommand::new("init")
                .about("Create a new pack")
                .arg(Arg::new("pack").help("Pack name").required(true)),
        )
        .subcommand(
            ClapCommand::new("fill")
                .about("Add placeholder files to an existing pack")
                .arg(Arg::new("pack").help("Pack name").required(true)),
        )
        .subcommand(
            ClapCommand::new("adopt")
                .about("Move files into a pack, symlinking from original location")
                .arg(
                    Arg::new("files")
                        .help("Files to adopt (pack inferred from path)")
                        .required(true)
                        .num_args(1..)
                        .action(ArgAction::Append),
                )
                .arg(
                    Arg::new("into")
                        .long("into")
                        .value_name("PACK")
                        .help("Force a destination pack (must already exist); overrides path-based inference")
                        .num_args(1),
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .help("Overwrite existing destination files in the pack")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .help("Show what would be adopted without making changes")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("no-follow")
                        .long("no-follow")
                        .help("When the source is a symlink, move the link itself instead of its target")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("only-os")
                        .long("only-os")
                        .value_name("LABEL")
                        .help(
                            "Wrap each adopted entry in a `_<label>/` gate dir so the deployed \
                             symlink only lands on hosts matching the gate predicate. \
                             Labels: darwin, linux, macos, arm64, aarch64, x86_64 (built-in) \
                             plus any user-defined entry in [gates]."
                        )
                        .num_args(1),
                ),
        )
        .subcommand(
            ClapCommand::new("addignore")
                .about("Mark a pack as pack-ignored (drops a .dodotignore marker)")
                .arg(Arg::new("pack").help("Pack name").required(true)),
        )
        .subcommand(
            config_cmd
                .as_command("config")
                .about("Manage configuration"),
        )
        .subcommand(
            ClapCommand::new("init-sh").about("Print shell init script for eval in .zshrc/.bashrc"),
        )
        .subcommand(
            ClapCommand::new("install")
                .about("Wires dodot into your shell startup; --write applies it")
                .arg(
                    Arg::new("write")
                        .long("write")
                        .help(
                            "Write the hook block to the startup file (idempotent), then verify \
                             it by starting a shell. Without this flag the command only reports.",
                        )
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("rc")
                        .long("rc")
                        .value_name("FILE")
                        .help("Use this startup file instead of the one dodot would pick")
                        .num_args(1),
                ),
        )
        .subcommand(
            ClapCommand::new("git-show-alias")
                .about(
                    "Print the Tier 2 shell alias that wraps git in `dodot refresh --quiet`, \
                     so `git status` and `git diff` see deployed-side template edits.",
                )
                .arg(
                    Arg::new("shell")
                        .long("shell")
                        .help("Target shell (bash, zsh). Auto-detected from $SHELL by default.")
                        .value_name("SHELL")
                        .num_args(1),
                ),
        )
        .subcommand(
            ClapCommand::new("git-install-alias")
                .about(
                    "Write the Tier 2 shell alias to your shell's rc file (~/.bashrc or \
                     ~/.zshrc). Idempotent and additive.",
                )
                .arg(
                    Arg::new("shell")
                        .long("shell")
                        .help("Target shell (bash, zsh). Auto-detected from $SHELL by default.")
                        .value_name("SHELL")
                        .num_args(1),
                ),
        )
        .subcommand(
            ClapCommand::new("template")
                .about(
                    "Template-source git integration: the clean filter (passthrough) and \
                     filter installer.",
                )
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    ClapCommand::new("clean")
                        .about(
                            "git clean filter for templates — reads source on stdin, writes \
                             patched form on stdout. Invoked by git when reading a template.",
                        )
                        .arg(
                            Arg::new("path")
                                .long("path")
                                .help("Working-tree path of the file being filtered (git's `%f`).")
                                .value_name("PATH")
                                .required(true)
                                .num_args(1),
                        ),
                )
                .subcommand(
                    ClapCommand::new("install-filter").about(
                        "Register the dodot-template clean filter in the dotfiles repo's \
                         .git/config (idempotent, per-clone, per-machine).",
                    ),
                ),
        )
        .subcommand(
            ClapCommand::new("plist")
                .about("Plist clean/smudge filters (stdin → stdout)")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    ClapCommand::new("clean")
                        .about("Convert any plist on stdin to canonical XML on stdout"),
                )
                .subcommand(
                    ClapCommand::new("smudge")
                        .about("Convert XML plist on stdin to binary plist on stdout"),
                ),
        )
        .subcommand(
            ClapCommand::new("git-install-filters")
                .about("Install plist clean/smudge filters into the dotfiles repo's .git/config"),
        )
        .subcommand(
            ClapCommand::new("git-show-filters")
                .about("Print the .git/config + .gitattributes snippets for plist filters"),
        )
        .subcommand(
            ClapCommand::new("prompts")
                .about("Inspect and reset dismissed-prompt state")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    ClapCommand::new("list")
                        .about("Show every known prompt with its dismissed/active state"),
                )
                .subcommand(
                    ClapCommand::new("reset")
                        .about("Clear a dismissed prompt so it fires again next time")
                        .arg(
                            Arg::new("key")
                                .help("Prompt key to reset (omit when using --all)")
                                .num_args(0..=1),
                        )
                        .arg(
                            Arg::new("all")
                                .long("all")
                                .help("Reset every dismissed prompt")
                                .action(ArgAction::SetTrue),
                        ),
                ),
        )
        .subcommand(
            ClapCommand::new("roots")
                .about("Inspect and revoke the dotfiles roots you have approved")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    ClapCommand::new("list").about(
                        "Show every approved dotfiles root, and whether it still exists",
                    ),
                )
                .subcommand(
                    ClapCommand::new("forget")
                        .about(
                            "Revoke one approval, so the next mutating command run from \
                             that root asks for confirmation again",
                        )
                        .arg(
                            Arg::new("path")
                                // OsString, not String: a non-Unicode root must
                                // survive from argv to the matching rules
                                // untouched, and `roots list` prints exactly
                                // what this accepts back.
                                .value_parser(clap::value_parser!(std::ffi::OsString))
                                .help(
                                    "The root to forget, as `dodot roots list` prints it \
                                     (an existing path is canonicalized first)",
                                )
                                .required(true),
                        ),
                ),
        )
        .subcommand(
            ClapCommand::new("refresh")
                .about(
                    "Touch template-source mtimes when deployed files have drifted, so `git \
                     status` and `git diff` re-read them and surface the changes.",
                )
                .arg(
                    Arg::new("quiet")
                        .long("quiet")
                        .help("Suppress all output (use this in shell aliases).")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("list-paths"),
                )
                .arg(
                    Arg::new("list-paths")
                        .long("list-paths")
                        .help(
                            "Print the source paths that need a touch and exit, without \
                             writing any mtimes. Use this in editor / file-watcher integrations.",
                        )
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            ClapCommand::new("transform")
                .about("Reverse-merge edits from deployed files back to template sources")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    ClapCommand::new("check")
                        .about(
                            "Check every preprocessed file for divergence and apply reverse-merge",
                        )
                        .arg(
                            Arg::new("strict")
                                .long("strict")
                                .help(
                                    "Also fail if any source carries unresolved \
                                     dodot-conflict markers (used by the pre-commit hook).",
                                )
                                .action(ArgAction::SetTrue),
                        )
                        .arg(
                            Arg::new("dry-run")
                                .long("dry-run")
                                .help(
                                    "Report what would be patched without writing to source files.",
                                )
                                .action(ArgAction::SetTrue),
                        ),
                )
                .subcommand(
                    ClapCommand::new("install-hook").about(
                        "Install the pre-commit hook that runs `dodot transform check --strict` \
                         on every commit. Idempotent and additive — preserves any existing hook.",
                    ),
                )
                .subcommand(
                    ClapCommand::new("status").about(
                        "Show the current state of every cached preprocessed file (synced / \
                         output_changed / input_changed / both_changed / missing_source / \
                         missing_deployed). Read-only.",
                    ),
                ),
        )
        .subcommand(
            ClapCommand::new("secret")
                .about("Inspect secret providers and template references (Phase S5)")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    ClapCommand::new("probe").about(
                        "Run probe() on every configured provider and report each \
                         outcome. Read-only. Always exits 0 — even a failing provider \
                         lineup isn't an error here, just information.",
                    ),
                )
                .subcommand(
                    ClapCommand::new("list").about(
                        "Enumerate every `secret(...)` call across the repo's \
                         templates. Read-only. Useful before the first `dodot up` \
                         to inventory which providers a repo needs.",
                    ),
                ),
        )
        .subcommand(
            ClapCommand::new("reset")
                .about("Resets all dodot setup, useful for troubleshooting")
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .help("Show what would be removed without making changes")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .help("Skip the confirmation prompt (required when stdin is not a terminal)")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            ClapCommand::new("tutorial")
                .about("Interactive walkthrough using your real dotfiles")
                .arg(
                    Arg::new("reset")
                        .long("reset")
                        .help("Discard saved tutorial state and start over")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("from")
                        .long("from")
                        .help("Jump to a specific step (intro|check_root|pick_pack|...)")
                        .value_name("STEP")
                        .num_args(1),
                ),
        )
        .subcommand(
            ClapCommand::new("probe")
                .about("Introspect deployed state (deployment map, data directory)")
                .subcommand_required(false)
                .arg_required_else_help(false)
                .subcommand(
                    ClapCommand::new("deployment-map")
                        .about("Show the source↔deployed map"),
                )
                .subcommand(
                    ClapCommand::new("show-data-dir")
                        .about("Tree view of dodot's data directory")
                        .arg(
                            Arg::new("depth")
                                .long("depth")
                                .help("Maximum tree depth (default 4)")
                                .value_parser(clap::value_parser!(usize))
                                .num_args(1),
                        ),
                )
                .subcommand(
                    ClapCommand::new("app")
                        .about(
                            "Introspect macOS app-support routing for a pack (folders, casks, bundles)",
                        )
                        .arg(
                            Arg::new("pack")
                                .help("Pack name to probe")
                                .value_name("PACK")
                                .required(true)
                                .num_args(1),
                        )
                        .arg(
                            Arg::new("refresh")
                                .long("refresh")
                                .help("Invalidate the brew cache for this pack's tokens before probing")
                                .action(ArgAction::SetTrue),
                        ),
                )
                .subcommand(
                    ClapCommand::new("shell-init")
                        .about(
                            "Startup timings + fresh hook verification (--trace-hook for PATH diagnosis)",
                        )
                        .arg(
                            Arg::new("filter")
                                .help(
                                    "Drill into one pack or file (e.g. `gpg` or `gpg/env.sh`) — shows per-run exit codes and captured stderr across recent runs; suppresses the trace",
                                )
                                .value_name("PACK[/FILE]")
                                .num_args(0..=1)
                                .conflicts_with("runs")
                                .conflicts_with("history"),
                        )
                        .arg(
                            Arg::new("no-verify")
                                .long("no-verify")
                                .help(
                                    "Skip fresh hook verification (no shell is spawned; report the recorded timings only)",
                                )
                                .action(ArgAction::SetTrue),
                        )
                        .arg(
                            Arg::new("no-trace")
                                .long("no-trace")
                                .help(
                                    "Deprecated alias for --no-verify",
                                )
                                .action(ArgAction::SetTrue),
                        )
                        .arg(
                            Arg::new("trace-hook")
                                .long("trace-hook")
                                .help(
                                    "Run the full hook-line PATH diagnosis instead of targeted verification (spawns your shell, running your whole rc file up to twice)",
                                )
                                .action(ArgAction::SetTrue)
                                .conflicts_with("no-verify")
                                .conflicts_with("no-trace"),
                        )
                        .arg(
                            Arg::new("runs")
                                .long("runs")
                                .help(
                                    "Aggregate the last N runs into per-target p50/p95/max (defaults to 10 if N is omitted)",
                                )
                                .value_parser(clap::value_parser!(usize))
                                // Optional value: `--runs` alone uses
                                // DEFAULT_RUNS, `--runs 5` overrides.
                                .num_args(0..=1)
                                .default_missing_value("10")
                                .conflicts_with("history"),
                        )
                        .arg(
                            Arg::new("history")
                                .long("history")
                                .help("Show one summary row per recent run, newest first")
                                .action(ArgAction::SetTrue),
                        )
                        .arg(
                            Arg::new("errors-only")
                                .long("errors-only")
                                .help(
                                    "List every target with a non-zero exit across recent runs, sorted by failure count",
                                )
                                .action(ArgAction::SetTrue)
                                .conflicts_with("runs")
                                .conflicts_with("history")
                                .conflicts_with("filter"),
                        ),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::pack_status_width;

    /// Every leaf command the CLI has, as dotted dispatch paths — the rows
    /// the command-policy matrix must classify.
    fn every_leaf_path() -> Vec<String> {
        let mut leaves = Vec::new();
        for command in super::build_clap_command().get_subcommands() {
            let name = command.get_name();
            leaves.extend(
                command
                    .get_subcommands()
                    .map(|sub| format!("{name}.{}", sub.get_name())),
            );
            // A grouping command that insists on a subcommand (`dodot
            // roots`) never dispatches on its own; one that does not
            // (`dodot probe`) has a bare form to classify too.
            if !command.is_subcommand_required_set() {
                leaves.push(name.to_string());
            }
        }
        leaves
    }

    /// The command-policy matrix: every command the CLI has carries exactly
    /// one explicit root-sensitivity classification.
    ///
    /// A dispatched command must have a row in
    /// `safety::COMMAND_SENSITIVITY` — undeclared means unhooked, which
    /// means the handler gets no resolved root and fails at its first step.
    /// A route `main` dispatches itself must have a row in
    /// `safety::PASSTHROUGH_POLICY` — there is no hook to fail loudly for
    /// it, so this test is what keeps a new passthrough from quietly
    /// escaping the gate. Either way, a new command cannot inherit "not
    /// gated" from whichever helper it called
    /// (`docs/adr/0002-guard-root-derived-mutations.md`); it fails here
    /// rather than in front of a user.
    #[test]
    fn every_command_carries_exactly_one_root_sensitivity_classification() {
        let app = super::build_app();

        for path in every_leaf_path() {
            let hooked = app.get_hooks(&path).is_some();
            let passthrough = crate::safety::passthrough_policy_covers(&path);
            assert!(
                hooked || passthrough,
                "`{path}` is in neither safety::COMMAND_SENSITIVITY nor \
                 safety::PASSTHROUGH_POLICY — classify it before it ships"
            );
            assert!(
                !(hooked && passthrough),
                "`{path}` is classified in both tables; one of them is dead policy"
            );
        }
    }

    /// The other direction: a row naming a command that no longer exists is
    /// dead policy, and dead policy is how a taxonomy stops being reviewable.
    #[test]
    fn every_declared_command_is_one_the_cli_has() {
        let command = super::build_clap_command();

        let declared = crate::safety::COMMAND_SENSITIVITY
            .iter()
            .map(|(path, _)| *path)
            .chain(
                crate::safety::PASSTHROUGH_POLICY
                    .iter()
                    .map(|(path, _)| *path),
            )
            // `help` is clap's synthesized subcommand plus main's own
            // pre-scan; neither appears in the unbuilt command tree.
            .filter(|path| *path != "help");

        for path in declared {
            let mut segments = path.split('.');
            let top = segments.next().expect("a path has at least one segment");
            let found = command.find_subcommand(top);
            assert!(found.is_some(), "`{path}` names no dodot command");
            if let (Some(parent), Some(leaf)) = (found, segments.next()) {
                assert!(
                    parent.find_subcommand(leaf).is_some(),
                    "`{path}` names no dodot command"
                );
            }
        }
    }

    #[test]
    fn pack_status_width_uses_standout_value_with_80_column_fallback() {
        assert_eq!(pack_status_width(Some(132)), 132);
        assert_eq!(pack_status_width(None), 80);
    }
}
