Architecture

    This document describes dodot's technical architecture and implementation details.

1. Core Concepts

    Dotfiles Root:
        Base directory containing all dotfile packs (defaults to ~/dotfiles).

    Pack:
        A directory containing related dotfiles (vim/, git/, zsh/).

    Rule:
        Pattern-to-handler mapping with priority.

    Handler:
        Converts matched files to operations (symlink, install, homebrew, shell, path).

    Operation:
        Atomic unit of work (CreateDataLink, CreateUserLink, RunCommand, CheckSentinel).

    DataStore:
        Minimal 8-method API for state management.

2. Processing Pipeline

    dodot follows a unified execution pipeline through `packs::orchestration::execute()`.

    Pipeline:

        Commands -> packs::orchestration::execute() -> rules -> handlers -> intents -> operations -> DataStore

    :: text ::

    2.1. Unified Execution Flow

        The `packs::orchestration::execute()` function provides a single entry point for all pack-based commands:

        - Pack Discovery: scans DOTFILES_ROOT for packs
        - Command Execution: executes the command for each pack via `execute_for_pack()`
        - Result Aggregation: collects and reports results across all packs

        Each command implementation decides how to process its pack:

        - Rule matching and handler execution (for up/down commands)
        - Direct file operations (for adopt/init/fill)
        - Status checking (for status command)

    2.2. Command Execution Details

        Each command implementation handles its own logic.

        Up Command Flow:
            - Rule Matching: scans pack files against rules
            - Handler Grouping: groups matches by handler
            - Operation Generation: handlers convert matches to HandlerIntents
            - Execution: executor converts intents to DataStore calls

        Down Command Flow:
            - State Discovery: lists handlers with state for the pack
            - State Removal: calls RemoveState for each handler

        Status Command Flow:
            - Rule Matching: scans pack files against rules
            - Status Checking: handlers check their own status via StatusChecker
            - Display: shows current state for each file

3. State Management

    dodot uses a "double-link" architecture for state.

    Double-link:

        Repository File -> DataStore Link -> Target Location
        ~/dotfiles/vim/.vimrc -> ~/.local/share/dodot/data/vim/symlink/.vimrc -> ~/.vimrc

    :: text ::

    This provides:

    - State representation without databases
    - Conflict detection before operations
    - Clean uninstall by removing intermediate layer
    - Operation tracking through sentinel files

4. Implementation Details

    4.1. Crate Organization

        Workspace layout:

            dodot-lib/src/           # Core library crate (no terminal deps)
            +-- commands/            # Public API: up, down, status, list, init, fill, adopt, addignore, genconfig
            +-- config/              # DodotConfig via clapfig/confique
            +-- datastore/           # DataStore trait + FilesystemDataStore
            +-- execution/           # Executor: intent -> DataStore dispatch
            +-- fs/                  # Fs trait + OsFs
            +-- handlers/            # symlink, shell, path, install, homebrew
            +-- operations/          # Operation enum, HandlerIntent
            +-- packs/               # Pack, discovery, orchestration pipeline
            +-- paths/               # Pather trait + XdgPather
            +-- render/              # standout theme + templates
            +-- rules/               # Rule, Scanner, pattern matching
            +-- shell/               # init script generation
            +-- testing/             # TempEnvironment builder

            dodot-cli/src/           # Thin CLI layer crate
            +-- main.rs              # clap command definitions + standout App wiring
            +-- handlers.rs          # Thin wrappers calling dodot-lib
            +-- templates/           # MiniJinja .jinja files
            +-- styles/              # CSS stylesheet

        :: text ::

    4.2. Key Types

        Rule:

            pub struct Rule {
                pub pattern: String,                       // e.g., "*.sh", "bin/", "!*.tmp"
                pub handler: String,                       // handler name
                pub priority: i32,                         // higher priority matches first
                pub options: HashMap<String, toml::Value>, // handler-specific options
            }

        :: rust ::

        Handler:

            pub trait Handler {
                fn name(&self) -> &str;
                fn category(&self) -> HandlerCategory;
                fn to_intents(&self, matches: &[RuleMatch]) -> Result<Vec<HandlerIntent>>;
                fn check_status(&self, matches: &[RuleMatch], ds: &dyn DataStore) -> Result<Vec<Status>>;
            }

        :: rust ::

        HandlerIntent:

            pub enum HandlerIntent {
                Link { source: PathBuf, target: PathBuf },
                Stage { source: PathBuf },
                Run { command: String, sentinel: String },
            }

        :: rust ::

        Operation:

            pub enum Operation {
                CreateDataLink { pack: String, handler: String, source: PathBuf },
                CreateUserLink { datastore_path: PathBuf, user_path: PathBuf },
                RunCommand { pack: String, handler: String, command: String, sentinel: String },
                CheckSentinel { pack: String, handler: String, sentinel: String },
            }

        :: rust ::

        DataStore:

            pub trait DataStore {
                fn create_data_link(&self, pack: &str, handler: &str, source: &Path) -> Result<PathBuf>;
                fn create_user_link(&self, datastore_path: &Path, user_path: &Path) -> Result<()>;
                fn run_and_record(&self, pack: &str, handler: &str, command: &str, sentinel: &str) -> Result<()>;
                fn has_sentinel(&self, pack: &str, handler: &str, sentinel: &str) -> Result<bool>;
                fn remove_state(&self, pack: &str, handler: &str) -> Result<()>;
                fn has_handler_state(&self, pack: &str, handler: &str) -> Result<bool>;
                fn list_pack_handlers(&self, pack: &str) -> Result<Vec<String>>;
                fn list_handler_sentinels(&self, pack: &str, handler: &str) -> Result<Vec<String>>;
            }

        :: rust ::

5. Rules System

    The rules system provides simple, declarative file matching.

    5.1. Pattern Conventions

        - `install.sh`: exact filename match
        - `*.sh`: glob pattern match
        - `bin/`: directory match (trailing slash)
        - `!*.tmp`: exclusion rule (leading !)
        - `*`: catchall pattern

    5.2. Rule Processing

        - Exclusions first: files matching !patterns are skipped
        - Priority order: higher priority rules match first
        - First match wins: once matched, no further rules apply

    5.3. Example Configuration

        Rules config:

            [[rules]]
            pattern = "install.sh"
            handler = "install"
            priority = 90

            [[rules]]
            pattern = "*.sh"
            handler = "shell"
            priority = 80

            [[rules]]
            pattern = "*"
            handler = "symlink"
            priority = 0

        :: toml ::

6. CLI Architecture

    The CLI layer follows strict design principles.

    6.1. Thin CLI Layer

        - Commands only parse arguments and call dodot-lib command functions
        - Business logic lives in `dodot_lib::commands` and handlers
        - No direct handler knowledge in CLI commands
        - Clear boundaries between layers

    6.2. Command Flow

        Handler-based commands:

            // Handler-based commands build a production context and call command functions
            let ctx = ExecutionContext::production(config, fs, datastore, pather);
            let result = commands::up::up(&ctx, &pack_names)?;

        :: rust ::

        Non-handler commands:

            // Non-handler commands use domain methods
            commands::adopt::adopt(&ctx, source_file, target_name)?;
            datastore.remove_state(pack, handler)?;

        :: rust ::

    6.3. Crate Organization

        CLI crate:

            dodot-cli/src/
            +-- main.rs         # clap command definitions + standout App wiring
            +-- handlers.rs     # Thin wrappers calling dodot-lib command functions
            +-- templates/      # MiniJinja .jinja files for output rendering
            +-- styles/         # CSS stylesheet for standout themes

        :: text ::

        Library crate:

            dodot-lib/src/
            +-- commands/       # Command implementations
            |   +-- up.rs       # Implements orchestration::Command trait
            |   +-- down.rs
            |   +-- status.rs
            |   +-- ...

        :: text ::

    6.4. Unified Output System

        The CLI uses *standout* for all output rendering. The `App::builder()` wires MiniJinja templates from `templates/` and CSS stylesheets from `styles/` into a unified rendering pipeline. The `--output` flag selects the output format (terminal, json, text).

7. Component Responsibilities

    7.1. CLI Commands (`dodot-cli`)

        Role:
            Command structure and help text.

        Responsibilities:
            Define clap commands with usage/examples, wire standout App for output rendering.

        What they don't do:
            Contain any business logic.

    7.2. Command Implementations (`dodot_lib::commands`)

        Role:
            Business logic for pack commands.

        Responsibilities:
            Implement `orchestration::Command` trait.

        Handler commands:
            Execute operations via handlers.

        Non-handler commands:
            Use Pack methods or DataStore directly.

        What they don't do:
            Handle CLI parsing, know about clap.

    7.3. Packs/Orchestration (`dodot_lib::packs::orchestration`)

        Role:
            Unified execution pipeline.

        Key function:
            `packs::orchestration::execute()` orchestrates the entire flow.

        Responsibilities:
            Pack discovery, command execution per pack.

    7.4. Rules (`dodot_lib::rules`)

        Role:
            Bridge between matches and execution.

        Key type:
            `Scanner` scans pack files against rules and produces matches.

        Responsibilities:
            Group matches by handler, determine execution order, invoke handlers.

    7.5. Handlers (`dodot_lib::handlers`)

        Role:
            File-specific intent generators.

        Responsibilities:
            Convert file matches to HandlerIntents (symlink, homebrew, shell, path, install).

        Used by:
            Handler-related commands (up/down).

        Size:
            50-100 lines each, focused on single concern.

    7.6. Packs (`dodot_lib::packs`)

        Role:
            Pack management domain object.

        Responsibilities:
            Pack operations (init, fill, adopt, addignore), guardian methods for safe manipulation.

        Used by:
            Pack-related commands (init, fill, adopt, addignore).

    7.7. DataStore (`dodot_lib::datastore`)

        Role:
            Operations execution abstraction.

        Responsibilities:
            Execute operations via 8-method trait API, state management.

        Trait methods:
            create_data_link, create_user_link, run_and_record, has_sentinel, remove_state, has_handler_state, list_pack_handlers, list_handler_sentinels.

    7.8. Executor (`dodot_lib::execution`)

        Role:
            Intent-to-operation conversion and dispatch.

        Responsibilities:
            Convert HandlerIntents to Operations, dispatch Operations to DataStore, dry-run support.

8. Performance Considerations

    - Lazy evaluation: only process requested packs
    - Parallel discovery: scan packs concurrently
    - Minimal I/O: cache filesystem operations
    - Early termination: stop on first error in dry-run

9. Architectural Principles

    9.1. Unified Execution

        - Single entry point: all pack-based commands use `packs::orchestration::execute()`
        - No business logic in CLI: commands are thin orchestrators only
        - Proper abstractions: Commands, Orchestration, Rules, Handlers, Intents, Operations, DataStore
        - No bypassing: never skip abstraction layers or access handlers directly

    9.2. Separation of Concerns

        - Handler commands: up, down use `packs::orchestration::execute()` for handler operations
        - Pack commands: init, fill, adopt use Pack guardian methods
        - State commands: down uses `DataStore::remove_state()`
        - Query commands: list, status use discovery functions

10. Error Philosophy

    - Fail fast: stop on first error
    - Clear messages: include context and suggestions
    - Error codes: stable identifiers for testing
    - Recovery hints: tell users how to fix issues
