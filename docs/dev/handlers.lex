Handlers

    This document is the contributor reference for the handler subsystem: the trait, the classification axes, the execution-order machinery, the data layout each handler writes to the datastore, and the registry. For the user-facing summary of which handler claims what, see [./../user/handlers.md]. For the conceptual overview of how handlers fit between rules and execution, see [./../reference/handlers.lex].

    :: note :: See [./../reference/terms-and-concepts.lex] for terminology used throughout.

1. Module Layout

    Handlers live in `crates/dodot-lib/src/handlers/`:

        handlers/
        +-- mod.rs           # Handler trait + classification enums + registry
        +-- filter.rs        # IgnoreHandler, SkipHandler (Filter phase)
        +-- symlink.rs       # SymlinkHandler (catchall, Link phase)
        +-- shell.rs         # ShellHandler (ShellInit phase)
        +-- path.rs          # PathHandler (PathExport phase)
        +-- install.rs       # InstallHandler (Setup phase, code execution)
        +-- homebrew.rs      # HomebrewHandler (Provision phase, code execution)

    :: text ::

    Each handler is a small struct (often zero-sized) that implements the [`Handler`] trait. The trait is object-safe — handlers are stored as `Box<dyn Handler>` in a `HashMap<String, Box<dyn Handler>>` registry and dispatched by name at runtime.

2. The `Handler` Trait

    Handler trait:

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

    Handlers are *intent planners*. `to_intents` reads matched files and produces a list of [`HandlerIntent`] values that the executor will turn into operations. The `fs` argument is read-only — handlers may stat or enumerate matched directories, but must not write, delete, or rename anything. Mutations belong to the executor, which keeps planning idempotent and safe to re-run.

    `check_status` reports whether a single file has been deployed by this handler. It receives the datastore but no `Pather`, so it cannot recompute deploy paths; status reporting that needs the resolved user-side path calls into handler-specific helpers (e.g. `symlink::resolve_target`) directly.

3. Classification Axes

    Three enums classify a handler. Together they decide where it runs, how matches flow to it, and how it gets tracked.

    3.1. `ExecutionPhase`

        Each handler belongs to exactly one phase. The enum's *declaration order* is the execution order — `derive(Ord)` does the rest, and [`rules::handler_execution_order`] sorts handler-name groups by looking up each name's phase in the registry.

        ExecutionPhase:

            pub enum ExecutionPhase {
                Filter,       // ignore, skip (drop files before any deploying handler)
                Provision,    // homebrew
                Setup,        // install
                PathExport,   // path
                ShellInit,    // shell
                Link,         // symlink (catchall, always last)
            }

        :: rust ::

        Adding a handler is a deliberate design choice: which phase does it belong to? There is no alphabetical fallback between known handlers. The three invariants pinning the order:

        - The filter phase is always first. `ignore` and `skip` exist to keep matched files away from deploying handlers; running them later would let a precise mapping or the catchall claim a file the user said to drop.
        - The catchall phase is always last. `symlink` is the only `MatchMode::Catchall` handler — running it before any precise handler would let it claim files that belong elsewhere.
        - Code-execution phases run before configuration phases. `Provision` and `Setup` produce filesystem state (installed binaries, formulae, generated files) that later phases may reference.

    3.2. `HandlerCategory`

        Derived from phase: `Provision` and `Setup` are `CodeExecution`; the rest are `Configuration`.

        HandlerCategory:

            pub enum HandlerCategory {
                Configuration,   // symlink, shell, path
                CodeExecution,   // install, homebrew
            }

        :: rust ::

        The category drives two behaviors:

        - `--no-provision` skips `CodeExecution` handlers for one run. `Configuration` handlers still run.
        - `dodot up` wipes per-pack state for `Configuration` handlers before re-applying current source, so a deleted source file no longer leaves an orphan link. `CodeExecution` handler state (sentinels) must persist across `up` runs so install scripts and `brew bundle` aren't re-executed every time. [`configuration_handler_names`] is the helper that filters the registry by category.

    3.3. `MatchMode` and `HandlerScope`

        Two orthogonal classification axes for matching.

        MatchMode and HandlerScope:

            pub enum MatchMode { Precise, Catchall }
            pub enum HandlerScope { Exclusive, Shared }

        :: rust ::

        `Precise` handlers claim only whitelisted patterns (`install.sh`, `Brewfile`, `bin/`). `Catchall` handlers take whatever precise handlers didn't.

        `Exclusive` handlers consume their match — no other handler sees the entry. `Shared` is reserved for future observer-style handlers (audit, indexing) that watch without deploying.

        At most one handler may be simultaneously `Catchall` and `Exclusive`. Two such handlers would race over leftovers with no principled tie-breaker. [`validate_registry`] enforces this with `debug_assert!` so the panic surfaces in dev builds; release builds tolerate the misconfiguration silently because the built-in registry is hard-coded and third-party handlers aren't loaded at runtime.

        Today's defaults: every built-in handler is `Exclusive`. Only `symlink` is `Catchall`.

4. Built-in Handlers

    Seven handlers ship in the registry. The table below is the canonical mapping of phase → category → match mode → scope.

    Handler classification:

        | Handler  | Phase       | Category       | Match mode | Scope     | Output intent       |
        | ignore   | Filter      | Configuration  | Precise    | Exclusive | (none)              |
        | skip     | Filter      | Configuration  | Precise    | Exclusive | (none)              |
        | homebrew | Provision   | CodeExecution  | Precise    | Exclusive | `Run`               |
        | install  | Setup       | CodeExecution  | Precise    | Exclusive | `Run`               |
        | path     | PathExport  | Configuration  | Precise    | Exclusive | `Stage`             |
        | shell    | ShellInit   | Configuration  | Precise    | Exclusive | `Stage`             |
        | symlink  | Link        | Configuration  | Catchall   | Exclusive | `Link`              |

    :: table align=llllll ::

    Filter handlers (`ignore`, `skip`) claim matches but emit no `HandlerIntent` — `to_intents` returns `Ok(vec![])`. Their effect comes entirely from being matched first by the rules layer (priority 100 / 50, above precise mappings at 10 and the catchall at 0): once a filter handler claims a file, no other handler sees it. They're real registered handlers (not synthetic-name dispatch), so the matching model and config grammar stay uniform.

    4.1. `SymlinkHandler`

        File: `handlers/symlink.rs`. Catchall. Reads `RuleMatch::is_dir` to decide between wholesale (one link for the whole directory) and per-file mode (recurse and emit one link per file).

        Per-file mode is triggered by either:

        - The matched directory contains any file whose relative path matches a `protected_paths` entry, OR
        - The matched directory has any file whose relative path is a key in `[symlink.targets]`, OR
        - The matched directory is one of the escape-prefix dirs `_home` or `_xdg` (wholesale-linking these would bake the prefix into the deploy path).

        Per-file recursion uses `Fs::read_dir` (read-only) and applies `crate::rules::should_skip_entry` against `pack_ignore` so `.DS_Store`, `.git`, `*.swp` etc. don't slip through the fallback.

        Target resolution is centralized in `resolve_target(pack, rel_path, config, paths)`. Priority, highest first:

            0. Custom target from `[symlink.targets]` (absolute → as-is, relative → resolved from `$XDG_CONFIG_HOME`)
            1. `home.X` prefix (top-level files only) → `$HOME/.X`
            2. `_home/<rest>` → `$HOME/.<rest>`; `_xdg/<rest>` → `$XDG_CONFIG_HOME/<rest>` (escape hatches that skip pack namespacing)
            3. `force_home` config list — first path segment matched without leading dot
            4. Default: `$XDG_CONFIG_HOME/<display_name>/<rel_path>`

        The pack name is run through [`packs::display_name_for`] before being used in the default rule, so a pack `010-nvim` deploys to `~/.config/nvim/`, not `~/.config/010-nvim/`. The ordering prefix is on-disk only.

        See [./../reference/symlink-paths.lex] for the user-facing version of these rules.

    4.2. `ShellHandler`

        File: `handlers/shell.rs`. Stages files (not dirs) into `packs/<pack>/shell/`. The init script picks them up.

        Filters out directory entries (`!m.is_dir`) — the rule patterns are file globs but a `RuleMatch` carrying `is_dir` could only happen if a user wrote a pattern like `aliases/`, which the shell handler isn't designed to handle.

    4.3. `PathHandler`

        File: `handlers/path.rs`. Stages directories (not files) into `packs/<pack>/path/`. The init script prepends them to `$PATH`.

        Inverse filter: keeps only `m.is_dir` — a `bin` *file* in a pack root is meaningless to this handler. The default rule pattern is `bin/` (trailing slash forces directory-only matching at the rules layer); the file filter here is defense in depth.

    4.4. `InstallHandler`

        File: `handlers/install.rs`. Holds an `&dyn Fs` reference because checksumming the script content is part of intent generation. Emits one `HandlerIntent::Run` per matched file; the `executable` is picked from the script's extension by `interpreter_for(path)`:

        - `.zsh` → `"zsh"`
        - `.sh`, `.bash`, missing, or unknown extension → `"bash"`

        The arguments are `["--", "<absolute_path>"]`. The `--` ends option parsing so a script path that happens to start with `-` doesn't get interpreted as a flag.

        Sentinel format: `<filename>-<checksum>`, where `<filename>` is the script's basename (e.g. `install.sh`) and `<checksum>` is the first 16 hex chars of `SHA-256(file_contents)`. Editing the script changes the checksum, which produces a new sentinel name, which causes the handler to re-run automatically.

        Output handling lives in the runner, not the handler. [`ShellCommandRunner`] (`crates/dodot-lib/src/datastore/mod.rs`) spawns the child with piped stdio, drains stderr in a worker thread, and scans stdout line-by-line — surfacing `# status: <message>` lines as live progress markers, passing the rest through only when the runner is constructed with `verbose: true` (wired from the CLI `--verbose` flag through [`ExecutionContext::production`]). The leading comment block is read by [`FilesystemDataStore::run_and_record`] before the runner is invoked, via the `extract_header_block` helper. On non-zero exit, captured stderr is dumped to the user's stderr even when not verbose, so failures stay debuggable.

    4.5. `HomebrewHandler`

        File: `handlers/homebrew.rs`. Same shape as install — holds an `&dyn Fs` for content hashing, emits `HandlerIntent::Run`. The executable is hardcoded to `"brew"` and the arguments are `["bundle", "--file", "<absolute_path>"]`. Sentinel format matches install (`<filename>-<checksum>`); editing the Brewfile re-runs `brew bundle`.

    4.6. `IgnoreHandler` and `SkipHandler` (filter)

        File: `handlers/filter.rs`. Both are zero-sized structs whose `to_intents` returns `Ok(vec![])` — they claim matches but emit no work. The whole effect is positional: rules tagged with handler `"ignore"` carry priority 100 and rules tagged `"skip"` carry priority 50, so during scanning either filter rule matches before any precise mapping (10) or the catchall (0) gets a chance.

        Both `check_status` impls return a placeholder `HandlerStatus { deployed: false, message: "", handler: "ignore"|"skip", file }` to satisfy the trait, but nothing reads them. Filter status is computed *directly from rule matches* in [`commands::status`]: matches under handler `"ignore"` are dropped from the output entirely, matches under `"skip"` are rendered with `Health::Skipped` (label `"skipped"`, style `skipped`). This visibility split is the entire reason for the two handlers — the matching contract is identical.

        Both are `MatchMode::Precise` and `Configuration` category. They have no datastore footprint: `dodot up` does not wipe per-pack `ignore/`/`skip/` directories because those directories never exist.

5. The Registry

    [`create_registry(fs)`] builds a `HashMap<String, Box<dyn Handler>>` keyed by handler name. The `fs` reference is needed by `install` and `homebrew` for checksumming.

    Registry construction:

        let mut registry: HashMap<String, Box<dyn Handler>> = HashMap::new();
        registry.insert(HANDLER_IGNORE.into(),  Box::new(filter::IgnoreHandler));
        registry.insert(HANDLER_SKIP.into(),    Box::new(filter::SkipHandler));
        registry.insert(HANDLER_SYMLINK.into(), Box::new(symlink::SymlinkHandler));
        registry.insert(HANDLER_SHELL.into(),   Box::new(shell::ShellHandler));
        registry.insert(HANDLER_PATH.into(),    Box::new(path::PathHandler));
        registry.insert(HANDLER_INSTALL.into(), Box::new(install::InstallHandler::new(fs)));
        registry.insert(HANDLER_HOMEBREW.into(),Box::new(homebrew::HomebrewHandler::new(fs)));
        validate_registry(&registry);
        registry

    :: rust ::

    Well-known names are exported as constants — `HANDLER_IGNORE`, `HANDLER_SKIP`, `HANDLER_SYMLINK`, `HANDLER_SHELL`, `HANDLER_PATH`, `HANDLER_INSTALL`, `HANDLER_HOMEBREW`. Use these everywhere instead of string literals.

    The registry is hard-coded. Third-party handlers would be added via code, not user input. There is no plugin mechanism today — the trait is stable enough that writing a custom handler is straightforward, but loading them at runtime is an explicit non-goal.

6. Default Rule Mappings

    The default file-pattern → handler map lives in `config::MappingsSection`. [`config::mappings_to_rules`] converts it into the [`Rule`] set the scanner uses.

    Default mappings:

        | Handler  | Default pattern(s)                                                                          | Priority | Case-insensitive |
        | ignore   | each `[mappings] ignore` entry (default `[]`)                                               | 100      | no               |
        | skip     | `README`, `README.*`, `LICENSE`, `LICENSE.*`, `CHANGELOG`, `CHANGELOG.*`, …                  | 50       | yes              |
        | path     | `bin/` (directory; trailing slash auto-added)                                               | 10       | no               |
        | install  | `install.sh`, `install.bash`, `install.zsh`                                                 | 10       | no               |
        | shell    | `aliases.{sh,bash,zsh}`, `profile.{sh,bash,zsh}`, `login.{sh,bash,zsh}`, `env.{sh,bash,zsh}` | 10       | no               |
        | homebrew | `Brewfile`                                                                                  | 10       | no               |
        | symlink  | `*` (catchall)                                                                              | 0        | no               |

    :: table align=llll ::

    Priorities decide rule-evaluation order at the scanner: filter rules (`ignore` 100, `skip` 50) run before precise rules (10), and the symlink catchall (0) runs last. The "first match wins" rule then routes each entry to exactly one handler — so a file that hits an `ignore` pattern is dropped before any deploying handler sees it.

    Only `skip`'s default rules are case-insensitive (so `Readme` and `readme` hit the same rule as `README`). [`Scanner::match_entries`] checks the compiled rule set once per scan and only allocates a per-entry lowercased basename when at least one rule has `case_insensitive = true` — so a user who clears `mappings.skip = []` and adds no other CI rules pays nothing; the default config exercises the CI path because the `skip` defaults populate it. See [`Rule::case_insensitive`].

    User overrides come through `[mappings]` in `.dodot.toml`. The handler list is fixed (you can't add a new handler from config), but the patterns each handler claims are fully replaceable. See [./config-system.lex] for resolution layering.

7. Datastore Layout per Handler

    Each handler's state lives under `$XDG_DATA_HOME/dodot/packs/<pack>/<handler>/`. The shape of that directory is determined by which intent the handler emits.

    Per-handler state:

        | Handler  | Intent  | Datastore contents                                                              |
        | symlink  | `Link`  | one symlink per source: `packs/<pack>/symlink/<filename> -> <source>`           |
        | shell    | `Stage` | one symlink per source: `packs/<pack>/shell/<filename> -> <source>`             |
        | path     | `Stage` | one symlink per source dir: `packs/<pack>/path/<dirname> -> <source>`           |
        | install  | `Run`   | one sentinel per executed script: `packs/<pack>/install/<filename>-<checksum>`  |
        | homebrew | `Run`   | one sentinel per Brewfile: `packs/<pack>/homebrew/Brewfile-<checksum>`          |

    :: table align=lll ::

    For configuration handlers, the directory IS the state — writing into it enables the handler for that pack, deleting from it disables it. There is no separate ledger.

    For code-execution handlers, the sentinel IS the "did this run with this content?" record. Sentinel content is `completed|<unix_ts>` (one line); the filename carries the content-hash key. Deleting a sentinel by hand is a supported way to force one re-run.

    Symlink emits both halves of the double-link (data link + user-side link); shell and path emit only the data link, and the generated `dodot-init.sh` walks `packs/*/shell/` and `packs/*/path/` at shell startup. See [./storage.lex] for the full datastore reference and [./../reference/data-layer.lex] for the conceptual model.

8. Handler-Relevant Configuration

    Handlers don't see `DodotConfig` directly. They see [`HandlerConfig`], a narrow subset built by `DodotConfig::to_handler_config()`.

    HandlerConfig:

        pub struct HandlerConfig {
            pub force_home: Vec<String>,        // [symlink] force_home
            pub protected_paths: Vec<String>,   // [symlink] protected_paths
            pub targets: HashMap<String, String>,// [symlink.targets]
            pub auto_chmod_exec: bool,          // [path] auto_chmod_exec
            pub pack_ignore: Vec<String>,       // [pack] ignore
        }

    :: rust ::

    Only `symlink` and `path` actually read this struct today. `shell`, `install`, `homebrew`, `ignore`, and `skip` accept it for trait uniformity but ignore the contents — their behavior is fully determined by the matched files. The narrow surface keeps handlers from coupling to config keys they don't need.

9. Adding a New Handler

    The mechanical steps:

    1. Create `handlers/<name>.rs` with a struct implementing [`Handler`]. Pick a phase based on what the handler does — drop-only filtering belongs in `Filter`; code execution belongs in `Provision` or `Setup`; configuration belongs in `PathExport`, `ShellInit`, or `Link`.
    2. Export a name constant in `handlers/mod.rs` (`pub const HANDLER_<NAME>: &str = "<name>";`).
    3. Register it in [`create_registry`].
    4. If the handler should claim files by default, add a pattern to [`config::MappingsSection`] and emit the corresponding rule from [`config::mappings_to_rules`]. If the handler is opt-in (user must add a rule explicitly), skip this step.
    5. Decide whether `validate_registry` still passes. Two `Catchall` + `Exclusive` handlers will trip the `debug_assert!`.
    6. Update the user-facing handler doc ([./../user/handlers.md]) and the reference ([./../reference/handlers.lex]) — the phase enum is contributor-visible, but the handler itself surfaces in user docs as soon as it ships.

    :: note :: The trait surface is small (two methods carry behavior, three more are classification). A new handler is typically a few dozen lines plus tests against `TempEnvironment`.

10. Testing

    Two patterns dominate.

    Unit tests against mocks.
        Handlers that do pure intent generation (symlink target resolution, install interpreter selection) are tested with hand-built `RuleMatch` values and `HandlerConfig` overrides. No filesystem needed.

    Integration tests against `TempEnvironment`.
        Anything that reads the filesystem (symlink per-file recursion, install/homebrew checksumming) uses [`testing::TempEnvironment`] which builds a real temp directory with isolated home and datastore. The pattern is fluent: `.pack("vim").file("vimrc", "x").done().build()`. See [./types-and-structure.lex] §6 for details.

    The handler trait itself has compile-time object-safety checks in `mod.rs` (`assert_object_safe`, `assert_boxable`) plus tests asserting the registry has exactly one exclusive catchall and that phase ordering matches declaration order.
