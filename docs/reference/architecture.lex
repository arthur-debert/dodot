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

    dodot follows a unified execution pipeline through `orchestration.Execute()`.

    Pipeline:

        Commands -> orchestration.Execute() -> rules -> handlers -> operations -> DataStore

    :: text ::

    2.1. Unified Execution Flow

        The `orchestration.Execute()` function provides a single entry point for all pack-based commands:

        - Pack Discovery: scans DOTFILES_ROOT for packs
        - Command Execution: executes the command for each pack via `ExecuteForPack()`
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
            - Operation Generation: handlers convert matches to operations
            - Execution: operations execute through DataStore

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

    4.1. Package Organization

        Package layout:

            pkg/
            +-- packs/         # Pack discovery, commands, and orchestration
            |   +-- commands/  # Pack command implementations
            |   +-- orchestration/ # Pipeline execution
            +-- types/         # Type definitions and interfaces
            +-- rules/         # Rule-based matching system
            +-- handlers/      # Handler implementations (50-100 lines each)
            +-- operations/    # Operation types and executor
            +-- datastore/     # Minimal state management (8 methods)
            +-- dispatcher/    # Command dispatch logic
            +-- config/        # Configuration management

        :: text ::

    4.2. Key Types

        Rule:

            type Rule struct {
                Pattern  string                 // e.g., "*.sh", "bin/", "!*.tmp"
                Handler  string                 // handler name
                Priority int                    // higher priority matches first
                Options  map[string]interface{} // handler-specific options
            }

        :: go ::

        Handler:

            type Handler interface {
                Name() string
                Category() HandlerCategory
                ToOperations(matches []RuleMatch) ([]Operation, error)
            }

        :: go ::

        Operation:

            type Operation struct {
                Type    OperationType // CreateDataLink, CreateUserLink, RunCommand, CheckSentinel
                Pack    string
                Handler string
                Source  string
                Target  string
                Command string
            }

        :: go ::

        DataStore:

            type DataStore interface {
                CreateDataLink(pack, handlerName, sourceFile string) (string, error)
                CreateUserLink(datastorePath, userPath string) error
                RunAndRecord(pack, handlerName, command, sentinel string) error
                HasSentinel(pack, handlerName, sentinel string) (bool, error)
                RemoveState(pack, handlerName string) error
                HasHandlerState(pack, handlerName string) (bool, error)
                ListPackHandlers(pack string) ([]string, error)
                ListHandlerSentinels(pack, handlerName string) ([]string, error)
            }

        :: go ::

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

        - Commands only parse arguments and call the dispatcher
        - Business logic lives in `pkg/packs/commands` and handlers
        - No direct handler knowledge in CLI commands
        - Clear boundaries between layers

    6.2. Command Flow

        Handler-based commands:

            // Handler-based commands use orchestration.Execute()
            result, err := orchestration.Execute(command, packNames, orchestration.Options{
                DotfilesRoot: opts.DotfilesRoot,
                DryRun:       opts.DryRun,
                Force:        opts.Force,
                FileSystem:   filesystem.NewOS(),
                DataStore:    dataStore,
                Paths:        paths.New(),
            })

        :: go ::

        Non-handler commands:

            // Non-handler commands use domain methods
            pack.AdoptFile(sourceFile, targetName)  // For adopt command
            dataStore.RemoveState(pack, handler)     // For removal operations

        :: go ::

    6.3. Package Organization

        CLI layer:

            cmd/dodot/          # Cobra CLI layer
            +-- commands/       # CLI command definitions
            |   +-- up/         # Command structure and help
            |   +-- down/
            |   +-- status/
            |   +-- ...
            +-- root.go         # Command routing to dispatcher

        :: text ::

        Implementations:

            pkg/
            +-- dispatcher/     # Routes CLI to implementations
            +-- packs/
                +-- commands/   # Command implementations
                    +-- up.go       # Implements orchestration.Command
                    +-- down.go
                    +-- status.go
                    +-- ...

        :: text ::

    6.4. Unified Output System

        Output renderers:

            // Terminal output with colors/tables
            renderer := output.NewTerminalRenderer()

            // JSON output for scripting
            renderer := output.NewJSONRenderer()

            // Text output for logs
            renderer := output.NewTextRenderer()

        :: go ::

7. Component Responsibilities

    7.1. CLI Commands (`cmd/dodot/commands`)

        Role:
            Command structure and help text.

        Responsibilities:
            Define cobra commands with usage/examples.

        What they don't do:
            Contain any business logic.

    7.2. Dispatcher (`pkg/dispatcher`)

        Role:
            Bridge between CLI and command implementations.

        Responsibilities:
            Route CLI commands to appropriate implementations.

        Key function:
            Creates command instances based on command name. Decouples CLI from business logic implementations.

    7.3. Command Implementations (`pkg/packs/commands`)

        Role:
            Business logic for pack commands.

        Responsibilities:
            Implement `orchestration.Command` interface.

        Handler commands:
            Execute operations via handlers.

        Non-handler commands:
            Use Pack methods or DataStore directly.

        What they don't do:
            Handle CLI parsing, know about cobra.

    7.4. Packs/Orchestration (`pkg/packs/orchestration`)

        Role:
            Unified execution pipeline.

        Key function:
            `orchestration.Execute()` orchestrates the entire flow.

        Responsibilities:
            Pack discovery, command execution per pack.

    7.5. Rules (`pkg/rules`)

        Role:
            Bridge between matches and execution.

        Key function:
            `rules.ExecuteMatches()` converts matches to operations.

        Responsibilities:
            Group matches by handler, determine execution order, invoke handlers, execute via DataStore.

    7.6. Handlers (`pkg/handlers`)

        Role:
            File-specific operation generators.

        Responsibilities:
            Convert file matches to operations (symlink, homebrew, shell, path, install).

        Used by:
            Handler-related commands (up/down).

        Size:
            50-100 lines each, focused on single concern.

    7.7. Packs (`pkg/types/pack.go`)

        Role:
            Pack management domain object.

        Responsibilities:
            Pack operations (init, fill, adopt, addignore), guardian methods for safe manipulation.

        Used by:
            Pack-related commands (init, fill, adopt, addignore).

    7.8. DataStore (`pkg/datastore`)

        Role:
            Operations execution abstraction.

        Responsibilities:
            Execute operations via 8-method API, state management.

        Interface:
            CreateDataLink, CreateUserLink, RunAndRecord, HasSentinel, RemoveState, HasHandlerState, ListPackHandlers, ListHandlerSentinels.

    7.9. Operations and Executor

        Operations:
            Data structures with type, pack, handler, source, target, command.

        Executor:
            Orchestrate operation execution with dry-run support.

8. Performance Considerations

    - Lazy evaluation: only process requested packs
    - Parallel discovery: scan packs concurrently
    - Minimal I/O: cache filesystem operations
    - Early termination: stop on first error in dry-run

9. Architectural Principles

    9.1. Unified Execution

        - Single entry point: all pack-based commands use `orchestration.Execute()`
        - No business logic in CLI: commands are thin orchestrators only
        - Proper abstractions: Commands, Orchestration, Rules, Handlers, Operations, DataStore
        - No bypassing: never skip abstraction layers or access handlers directly

    9.2. Separation of Concerns

        - Handler commands: up, down use `orchestration.Execute()` for handler operations
        - Pack commands: init, fill, adopt use Pack guardian methods
        - State commands: down uses `DataStore.RemoveState()`
        - Query commands: list, status use discovery functions

10. Error Philosophy

    - Fail fast: stop on first error
    - Clear messages: include context and suggestions
    - Error codes: stable identifiers for testing
    - Recovery hints: tell users how to fix issues
