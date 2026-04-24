Types and Structure

    This document is the map for contributors. It covers the workspace layout, the module boundaries inside `dodot-lib`, and the key types that flow through the pipeline. For the conceptual view of the pipeline itself, see [./../reference/architecture.lex].

    :: note :: See [./../reference/terms-and-concepts.lex] for terminology used throughout.

1. Workspace Layout

    dodot is a Cargo workspace with two crates.

    Workspace:

        crates/
        +-- dodot-lib/        # core library (no terminal / CLI dependencies)
        +-- dodot-cli/        # thin CLI layer over the library

    :: text ::

    The split is intentional: all logic is testable without a CLI, and embedders could use `dodot-lib` directly if needed. `dodot-cli` is a few hundred lines of clap command definitions plus wrappers that call into `dodot-lib` and feed the results into standout for rendering.

2. Library Module Layout

    Top-level modules in `dodot-lib/src/`:

    Modules:

        commands/        # public API: up, down, status, list, init, fill, adopt, addignore
        config/          # DodotConfig + layered resolution via clapfig/confique
        conflicts.rs     # conflict detection utilities
        datastore/       # DataStore trait + FilesystemDataStore
        error.rs         # error types and codes
        execution/       # Executor: intent -> operation -> DataStore dispatch
        fs/              # Fs trait + OsFs (swappable for testing)
        handlers/        # built-in handlers: symlink, shell, path, install, homebrew
        operations/      # Operation enum, HandlerIntent enum
        packs/           # Pack discovery, orchestration, guardian methods
        paths/           # Pather trait + XdgPather
        preprocessing/   # preprocessor pipeline + preprocessors (template, unarchive, identity)
        render/          # standout theme + template registration
        rules/           # Rule, PackEntry, RuleMatch, Scanner
        shell/           # shell init script generation
        testing/         # TempEnvironment builder and test helpers
        lib.rs           # crate root, re-exports

    :: text ::

    Dependencies go inward: commands depend on packs/handlers/execution; handlers depend on rules/operations; datastore and fs are at the bottom of the stack.

3. CLI Module Layout

    CLI modules in `dodot-cli/src/`:

    CLI modules:

        main.rs          # clap command definitions + standout App wiring
        handlers.rs      # thin wrappers calling into dodot-lib commands
        logging.rs       # verbosity setup, log directory
        templates/       # MiniJinja .jinja files embedded at compile time
        styles/          # CSS stylesheets for standout themes

    :: text ::

4. Key Types

    The pipeline is shaped by a small number of types. This section lists them in the order they appear during a `dodot up`.

    4.1. `Rule`

        Defined in `rules::Rule`. A declarative pattern-to-handler mapping with a priority.

        Rule:

            pub struct Rule {
                pub pattern: String,           // "install.sh", "*.sh", "bin/", "!*.tmp"
                pub handler: String,           // handler name
                pub priority: i32,             // higher wins
                pub options: HashMap<String, String>,
            }

        :: rust ::

        Patterns with `!` prefix are exclusions and run before positive matches. Equal priority falls back to first-match order.

    4.2. `PackEntry` and `RuleMatch`

        Also in `rules`. `PackEntry` is a raw directory entry discovered by the scanner. `RuleMatch` is a `PackEntry` that a rule claimed — it carries the entry plus the handler that matched it.

    4.3. `Handler`

        Defined in `handlers::Handler`. The trait every handler implements.

        Handler trait (abridged):

            pub trait Handler: Send + Sync {
                fn name(&self) -> &str;
                fn phase(&self) -> ExecutionPhase;
                fn category(&self) -> HandlerCategory { self.phase().category() }
                fn match_mode(&self) -> MatchMode { MatchMode::Precise }
                fn scope(&self) -> HandlerScope { HandlerScope::Exclusive }
                fn to_intents(
                    &self,
                    matches: &[RuleMatch],
                    config: &HandlerConfig,
                    paths: &dyn Pather,
                    fs: &dyn Fs,
                ) -> Result<Vec<HandlerIntent>>;
                fn check_status(
                    &self,
                    file: &Path,
                    pack: &str,
                    datastore: &dyn DataStore,
                ) -> Result<HandlerStatus>;
            }

        :: rust ::

        `phase` places the handler in the intra-pack execution order (see [./../reference/handlers.lex] for the phase list and ordering rationale). `category` is derived from phase — Configuration vs Code Execution — and drives `--no-provision`. `match_mode` and `scope` are the classification axes from the matching model. `to_intents` produces the handler's plan; `fs` is passed read-only for tasks like enumerating a matched directory to decide wholesale vs per-file linking. `check_status` reports whether a single file has been deployed by this handler.

        Execution phases:

            pub enum ExecutionPhase {
                Provision,   // homebrew
                Setup,       // install
                PathExport,  // path
                ShellInit,   // shell
                Link,        // symlink (catchall, always last)
            }

        :: rust ::

        The enum's *declaration order* is the execution order — `derive(Ord)` does the rest. `rules::handler_execution_order` sorts handler-name groups by looking up each name's phase in the registry and ordering on that. Adding a handler is a deliberate design choice: which phase does it belong to? There is no alphabetical fallback between known handlers.

    4.4. `HandlerIntent`

        Defined in `operations::HandlerIntent`. A handler's declarative output — one of three shapes.

        Intent:

            pub enum HandlerIntent {
                Link { pack, handler, source, user_path },
                Stage { pack, handler, source },
                Run { pack, handler, executable, arguments, sentinel },
            }

        :: rust ::

        `Link` is a full double-link deployment (used by symlink). `Stage` adds a source file to the datastore without a user-side link (used by shell and path; the init script picks them up). `Run` is a tracked command execution (used by install and homebrew). The `force` flag that overrides sentinel checks lives on `DataStore::run_and_record`, not on the intent.

    4.5. `Operation`

        Defined in `operations::Operation`. The concrete filesystem verb the executor dispatches to the datastore.

        Operation:

            pub enum Operation {
                CreateDataLink   { pack, handler, source },
                CreateUserLink   { pack, handler, datastore_path, user_path },
                RunCommand       { pack, handler, executable, arguments, sentinel },
                CheckSentinel    { pack, handler, sentinel },
            }

        :: rust ::

        A single `Link` intent produces one `CreateDataLink` plus one `CreateUserLink`. A `Stage` intent produces one `CreateDataLink`. A `Run` intent produces one `RunCommand` (with an implicit sentinel check).

    4.6. `DataStore`

        Defined in `datastore::DataStore`. The abstraction that executes operations. See [./storage.lex] for the full method list and filesystem layout.

    4.7. `Fs` and `Pather`

        Lower-level abstractions for filesystem access and path resolution. Both have `Os*` implementations for production and are swapped in `testing::TempEnvironment` for integration tests that need a real temp directory without touching the user's home.

5. Execution Context

    All handler-based commands run through an `ExecutionContext` (defined in `packs::orchestration`). The context carries:

    - A `ConfigManager` for config resolution
    - An `Fs` implementation
    - A `DataStore` implementation
    - A `Pather` for XDG resolution

    Production code uses `ExecutionContext::production(dotfiles_root)` which wires the real implementations. Tests use `TempEnvironment` to build one with temp directories.

6. Testing Infrastructure

    `dodot-lib::testing` provides the `TempEnvironment` builder used by integration tests. It creates a real temp directory, sets up an isolated dotfiles root and datastore, lets you populate pack files fluently, and exposes `fs`, `paths`, `home`, and `dotfiles_root` fields plus assertion helpers like `assert_symlink`.

    Pattern:

        let env = TempEnvironment::builder()
            .pack("vim")
                .file("vimrc", "set number")
                .file("bin/script.sh", "#!/bin/sh\necho hi")
                .done()
            .build()?;
        // use env.fs, env.paths, env.dotfiles_root to drive commands
        env.assert_symlink("~/.vimrc")?;

    :: rust ::

    Unit tests that don't need real filesystem operations can mock `Fs` and `DataStore` directly; both traits are small.
