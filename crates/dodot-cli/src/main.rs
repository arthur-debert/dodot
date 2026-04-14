use clap::{Arg, ArgAction, Command as ClapCommand};
use standout::cli::App;

mod handlers;

fn main() {
    let cmd = build_clap_command();

    let app = App::builder()
        .templates(standout::embed_templates!("src/templates"))
        .styles(standout::embed_styles!("src/styles"))
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
        .command("adopt", handlers::adopt_handler, "message")
        .expect("register adopt")
        .command("addignore", handlers::addignore_handler, "message")
        .expect("register addignore")
        .command("genconfig", handlers::genconfig_handler, "config")
        .expect("register genconfig")
        .build()
        .expect("app build");

    let handled = app.run(cmd, std::env::args());
    if !handled {
        let _ = build_clap_command().print_help();
        println!();
    }
}

fn build_clap_command() -> ClapCommand {
    ClapCommand::new("dodot")
        .about("A dotfiles manager that uses symlinks for live editing")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand(
            ClapCommand::new("status")
                .about("Show deployment status of packs")
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
                        .help("Overwrite existing files in pack")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            ClapCommand::new("addignore")
                .about("Mark a pack as ignored")
                .arg(Arg::new("pack").help("Pack name").required(true)),
        )
        .subcommand(
            ClapCommand::new("genconfig")
                .about("Generate default configuration")
                .arg(
                    Arg::new("write")
                        .long("write")
                        .help("Write config to dotfiles root")
                        .action(ArgAction::SetTrue),
                ),
        )
}
