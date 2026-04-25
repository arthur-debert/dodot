use clap::{Arg, ArgAction, Command as ClapCommand};
use standout::cli::{App, CommandGroup};
use standout::EmbeddedTemplates;

use dodot_lib::render;

mod handlers;
mod logging;

fn main() {
    let app = build_app();

    // parse_with handles help rendering (with command groups) and exits if help requested
    let matches = app.parse_with(build_clap_command());

    // Initialize logging based on CLI flags
    let verbosity = if matches.get_flag("debug") {
        logging::Verbosity::Debug
    } else if matches.get_flag("verbose") {
        logging::Verbosity::Verbose
    } else {
        logging::Verbosity::Quiet
    };
    let log_dir = dodot_lib::paths::XdgPather::from_env()
        .map(|p| dodot_lib::paths::Pather::log_dir(&p))
        .unwrap_or_else(|_| std::env::temp_dir().join("dodot-logs"));
    let _log_guard = logging::init(&log_dir, verbosity);

    // Passthrough: config (clapfig handles its own output)
    if let Some(("config", sub_matches)) = matches.subcommand() {
        // If no config subcommand given, show config help instead of
        // falling through to config list (#20)
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
            eprintln!("error: {e}");
            std::process::exit(1);
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

    // All other commands go through standout dispatch
    let output_mode = app.extract_output_mode(&matches);
    match app.dispatch(matches, output_mode) {
        standout::cli::RunResult::Handled(output) => println!("{output}"),
        standout::cli::RunResult::Silent => {}
        standout::cli::RunResult::NoMatch(_) => {
            // No subcommand given — show help
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
    }
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
];

fn build_app() -> App {
    App::builder()
        .help_handling(true)
        .templates(EmbeddedTemplates::new(TEMPLATE_ENTRIES, ""))
        .styles(standout::embed_styles!("src/styles"))
        .default_theme("dodot")
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
                title: "Misc".into(),
                help: None,
                commands: vec![
                    Some("init-sh".into()),
                    Some("config".into()),
                    Some("help".into()),
                ],
            },
        ])
        .build()
        .expect("app build")
}

fn build_clap_command() -> ClapCommand {
    let config_cmd = clapfig::ConfigCommand::new();

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
                .arg(Arg::new("pack").help("Pack to adopt into").required(true))
                .arg(
                    Arg::new("files")
                        .help("Files to adopt")
                        .required(true)
                        .num_args(1..)
                        .action(ArgAction::Append),
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
                ),
        )
        .subcommand(
            ClapCommand::new("addignore")
                .about("Mark a pack as ignored")
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
                    ClapCommand::new("shell-init")
                        .about("Per-source timings for the most recent shell startup")
                        .arg(
                            Arg::new("runs")
                                .long("runs")
                                .help(
                                    "Aggregate the last N runs into per-target p50/p95/max",
                                )
                                .value_parser(clap::value_parser!(usize))
                                .num_args(1)
                                .conflicts_with("history"),
                        )
                        .arg(
                            Arg::new("history")
                                .long("history")
                                .help("Show one summary row per recent run, oldest first")
                                .action(ArgAction::SetTrue),
                        ),
                ),
        )
}
