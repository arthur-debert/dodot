Handlers

    This document is the contributor reference for the handler subsystem: the trait, the classification axes, the execution-order machinery, the data layout each handler writes to the datastore, and the registry. The roster itself — every registered handler with its phase, category, match mode, scope, and default patterns — is generated from the registry at [./../reference/handler-registry.lex], and this document links there rather than restating it. For the user-facing summary of which handler claims what, see [./../user/handlers/mappings.lex]. For the conceptual overview of how handlers fit between rules and execution, see [./../reference/handlers.lex].

    :: note :: See [./../reference/terms-and-concepts.lex] for terminology used throughout.

1. Module Layout

    Handlers live in `crates/dodot-lib/src/handlers/`:

        handlers/
        +-- mod.rs           # Handler trait + classification enums + registry
        +-- catalog.rs       # HandlerDoc rows + the generated-docs renderer
        +-- filter.rs        # IgnoreHandler, SkipHandler (Filter phase)
        +-- gate.rs          # GateHandler (Filter phase, scanner-minted matches)
        +-- externals.rs     # ExternalsHandler (External phase, code execution)
        +-- symlink.rs       # SymlinkHandler (catchall, Link phase)
        +-- shell.rs         # ShellHandler (ShellInit phase)
        +-- path.rs          # PathHandler (PathExport phase)
        +-- run_once.rs      # RunOnceCommand trait + RunOnceHandler<C>
        +-- install.rs       # InstallCommand (Setup phase, code execution)
        +-- homebrew.rs      # BrewfileCommand (Provision phase, code execution)
        +-- nix.rs           # NixCommand (Provision phase, code execution)

    :: text ::

    Most handlers are a small struct (often zero-sized) that implements the [`Handler`] trait. The three run-once provisioning handlers are the exception: `install`, `homebrew`, and `nix` are all instances of the single generic `RunOnceHandler<C>`, parameterized by the `RunOnceCommand` their module holds — see §4.4.

    The trait is object-safe — handlers are stored as `Box<dyn Handler>` in a `HashMap<String, Box<dyn Handler>>` registry and dispatched by name at runtime.

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
                Filter,       // gate, ignore, skip (drop files before any deploying handler)
                External,     // external (fetch declared remote resources)
                Provision,    // homebrew, nix
                Setup,        // install
                PathExport,   // path
                ShellInit,    // shell
                Link,         // symlink (catchall, always last)
            }

        :: rust ::

        A phase may hold more than one handler: `Filter` holds three, `Provision` holds two. The order two handlers sharing a phase run in is not defined — [`rules::handler_execution_order`] stable-sorts names collected from a `HashMap`, so their relative order is whatever the map iteration produced. Nothing in dodot depends on it.

        Adding a handler is a deliberate design choice: which phase does it belong to? There is no alphabetical fallback between known handlers.

        First, two things the phase order does *not* do, because who claims a file is settled before any phase is consulted. [`Scanner::match_entries`] hands each entry to exactly one handler — the highest-priority rule that matches it, or a `gate` match the scanner mints itself from host facts — and only those finished matches are then grouped by handler and sorted by phase. So a file the user said to drop is kept away from a precise mapping and the catchall by `ignore` at priority 100 and `skip` at 50, not by `Filter` running first; and `symlink` is kept off a `Brewfile` by its priority-0 catchall rule and the one-exclusive-catchall invariant [`validate_registry`] asserts, not by `Link` running last. Moving either phase would change nothing about who claims what.

        What the order does pin:

        - Code-execution phases run before configuration phases. `External`, `Provision`, and `Setup` produce filesystem state (fetched content, installed binaries, formulae, generated files) that later phases may reference.
        - `External` runs first among those three: install scripts and shell init may reference fetched content at its target path, so the fetch has to have happened.
        - `Filter` sits first and `Link` last so the phase list reads in the same order as the priority ladder. Nothing depends on either slot: the filter handlers emit no intents at all, and `symlink` only ever deploys what no other handler wanted.

    3.2. `HandlerCategory`

        Derived from phase: `External`, `Provision`, and `Setup` are `CodeExecution`; `Filter`, `PathExport`, `ShellInit`, and `Link` are `Configuration`.

        HandlerCategory:

            pub enum HandlerCategory {
                Configuration,   // gate, ignore, skip, path, shell, symlink
                CodeExecution,   // external, homebrew, nix, install
            }

        :: rust ::

        The category drives two behaviors:

        - `--no-provision` skips `CodeExecution` handlers for one run. `Configuration` handlers still run.
        - `dodot up` wipes per-pack state for `Configuration` handlers before re-applying current source, so a deleted source file no longer leaves an orphan link. `CodeExecution` handler state (sentinels) must persist across `up` runs so install scripts, `brew bundle`, and `nix profile install` aren't re-executed every time, and so a fetched external is not re-downloaded on every run. [`configuration_handler_names`] is the helper that filters the registry by category.

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

    Ten handlers ship in the registry: `gate`, `ignore`, `skip`, `external`, `homebrew`, `nix`, `install`, `path`, `shell`, `symlink`.

    The roster itself — every handler with its phase, category, match mode, scope, what it claims by default, and what it does with a claim — is generated from the registry at [./../reference/handler-registry.lex]. That page is the canonical mapping; this section covers only what a contributor needs beyond it, one subsection per handler implementation.

    Filter handlers (`ignore`, `skip`, `gate`) claim matches but emit no `HandlerIntent` — `to_intents` returns `Ok(vec![])`. Their effect comes entirely from being matched first: `ignore` and `skip` use static `[mappings]` patterns surfaced as priority-100 / priority-50 rules (above the precise mappings at 20 and 10 and the catchall at 0); `gate` matches are produced at scan time by `crate::gates` based on host facts (filename `._<label>`, dirname `_<label>/`, or `[mappings.gates]` glob hits) and stamped onto entries before rule matching runs. Once any of the three claims a file, no deploying handler sees it. All three are real registered handlers, not synthetic-name dispatch, so the matching model and config grammar stay uniform.

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

    4.4. The run-once handlers: `RunOnceCommand` + `RunOnceHandler`

        File: `handlers/run_once.rs`. `install`, `homebrew`, and `nix` are not three handler structs. There is one handler type, `RunOnceHandler<C>`, parameterized by a small [`RunOnceCommand`]; each of the three modules holds nothing but its command and that command's tests. `handlers/install.rs` holds `InstallCommand`, `handlers/homebrew.rs` holds `BrewfileCommand`, `handlers/nix.rs` holds `NixCommand`.

        All three do the same job — run a program against a user-provided file, hash the file, write a sentinel so the same content does not run twice — so that job is written once.

        4.4.1. What a command supplies, what the handler owns

            A `RunOnceCommand` supplies only what differs between the three:

            - `handler_name()` and `phase()` — identity.
            - `command_for(path) -> (executable, arguments)` — a pure function of the matched path. It names the executable the way a user would (`brew`, `nix`, `bash`); the planner substitutes the absolute path the availability probe found before the intent leaves it, so this never consults the environment.
            - `status_pending()` / `status_deployed()` / `status_ran_different()` — the copy for the three states.
            - `environment()` — defaulted, and the default is what all three use: it reads the handler's row in [`provisioners::PROVISIONERS`], so a variable a command needs is declared next to the argv it qualifies rather than branched on at the spawn site.

            `RunOnceHandler` owns everything else: skipping directory matches and matches with neither rendered bytes nor an on-disk file (the first-time-pack placeholder case), computing the checksum — preferring a preprocessor's in-memory rendered bytes over a disk read, which is what lets `status` and `up --dry-run` compute correct sentinels for templated files without materializing them — building the sentinel name, emitting `HandlerIntent::Run`, and mapping [`DataStore::did_run`] onto the three-state status row.

            It holds an `&dyn Fs` for that checksumming; it holds no `CommandRunner`, because nothing on the planning path spawns.

        4.4.2. The three specializations

            Per-command differences, in full:

                | Command          | Executable + arguments                                                                                  | Environment                  | Status copy (pending / deployed / older)                                                    |
                | `InstallCommand`  | `<interpreter> -- <abs path>`, interpreter from the extension (`.zsh` → `zsh`, anything else → `bash`) | (none)                       | `never run` / `installed` / `older version`                                                  |
                | `BrewfileCommand` | `brew bundle --no-upgrade --file <abs path>`                                                            | `HOMEBREW_NO_AUTO_UPDATE=1`  | `brew packages not installed` / `installed` / `brew packages older version`                  |
                | `NixCommand`      | `nix profile install --impure --extra-experimental-features "nix-command flakes" --argstr manifest <abs path> --expr <wrapper>` | (none) | `nix packages not installed` / `nix packages installed` / `nix packages older version`       |

            :: table align=llll ::

            The `--` in the install command ends option parsing, so a script path starting with `-` is not read as a flag. The interpreter comes from the extension rather than the user's login shell: the script runs in its own subprocess, so the interactive shell's aliases and options are irrelevant, and `install.zsh` is the pack author declaring zsh syntax.

            Two of brew's defaults are turned off, and each costs the user something otherwise. `--no-upgrade` keeps the run to what the `Brewfile` declares: `brew bundle` upgrades every outdated formula it meets by default, so an `up` could become a long mutating upgrade of packages the file never mentions. `HOMEBREW_NO_AUTO_UPDATE=1` stops the first brew invocation of the day from running a `brew update` first — several seconds of network traffic upgrading brew and its taps. The variable is not set at the spawn site: it is declared in the handler's [`provisioners`] descriptor row and carried by the `environment` field that runs the length of the provisioning path — `RunOnceCommand::environment` → [`HandlerIntent::Run`] → [`Operation::RunCommand`] → [`DataStore::run_and_record`] → [`CommandRunner::run`], which receives it as part of a [`CommandSpec`] and layers it onto the environment dodot itself inherited. A command declaring no variables (`install` and `nix` today) spawns a child with dodot's environment untouched.

            The nix command passes the manifest as `--argstr manifest <path>` and installs a single shape-normalizing wrapper expression rather than the file directly, so one invocation covers list, bare-derivation, and attribute-set manifests. `--impure` is unconditional — `--expr` evaluates in pure mode, which forbids reading an absolute path outside the store, and every manifest shape dodot documents needs to read both the manifest and `<nixpkgs>`. Each command's manifest argument sits at a position its `provisioners` row declares (`install` 1, `homebrew` 3, `nix` 7), because the datastore has to know which file the run was pointed at.

        4.4.3. Three states, and dodot never auto-reruns

            [`DataStore::did_run`] classifies a matched file three ways:

            - `NeverRan` — no sentinel for this file under any checksum. The handler emits a `Run` intent and the command executes.
            - `RanCurrent` — a sentinel whose recorded hash matches the current content. Skipped silently.
            - `RanDifferent` — a sentinel exists, but for a *different* content hash. Skipped *with a notice* (`older version`). The edit is not applied.

            Editing an `install.sh`, a `Brewfile`, or a `packages.nix` therefore changes what dodot *reports*, never what it runs. Applying the edit takes `dodot up --provision-rerun`, which bypasses both skip cases; that same flag is also how unchanged content is re-run deliberately. `--force` is unrelated — it only overwrites pre-existing files at symlink target paths. This holds for template-rendered content too: changing a template variable changes the rendered bytes and so the hash, which dodot detects and reports as `older version`.

            Two states come from outside the model. `--no-provision` drops code-execution handlers before they are consulted, and those files render as `skipped (--no-provision)`. An absent package manager is skipped by the planner before intent generation — see [`provisioners::availability`], which stats candidate paths and never spawns.

        4.4.4. Sentinels and snapshots

            Sentinel name: `<basename>-<checksum>`, where `<basename>` is the matched file's basename (`install.sh`, `Brewfile`, `packages.nix`) and `<checksum>` is the first 8 bytes of `SHA-256(content)` hex-encoded — 16 hex characters. The body is `completed|<unix-secs>`.

            [`DataStore::run_and_record`] also writes a `<sentinel>.snapshot` sibling holding the bytes that ran. That snapshot is what feeds the `(N lines added, M removed)` summary on an `older version` row and the body of `dodot status --diff`. Sentinels written before snapshots existed have no sibling, and their rows read `older version (no diff data)`.

            Deleting run-once state by hand is supported, and it takes *every* sentinel for that basename, not just the current one: [`DataStore::did_run`] answers `NeverRan` only when no `<basename>-<hash>` file is left in the handler's directory at all. `run_and_record` never removes the sentinels it supersedes, so after a few `--provision-rerun`s several hashes sit side by side; delete only the newest pair and the next plain `up` reads an older one and still reports `older version`. Remove `<basename>-*` and their `.snapshot` siblings together, or use `--provision-rerun`.

        4.4.5. Why there is no `validate` hook

            The trait deliberately has none. A finding raised inside `to_intents` would die with the intent, and `commands::status`'s run-once row is derived from the source file and the datastore without ever calling the handler — so an absent manager checked there would still have rendered as `never ran`. That question lives in [`provisioners::availability`], read by both the pack planner and status from one module.

            Per-content validation at planning time is out of scope on purpose. A malformed manifest must fail at apply time, the way `brew bundle` or `bash` fails. A validator that rejects on content would break the lifecycle every command shares: a file that ran once and was later edited into a broken state would fail planning instead of reaching the `older version` notice the run-once policy promises.

        4.4.6. Output handling

            Output handling lives in the runner, not the handler. [`ShellCommandRunner`] (`crates/dodot-lib/src/datastore/mod.rs`) spawns the child with piped stdio, drains stderr in a worker thread, and scans stdout line-by-line — surfacing `# status: <message>` lines as live progress markers, passing the rest through only when the runner is constructed with `verbose: true` (from the CLI `--verbose` flag through [`ExecutionContext::production`]). The matched file's leading comment block is read by [`FilesystemDataStore::run_and_record`] before the runner is invoked, via the `extract_header_block` helper. On non-zero exit, captured stderr is dumped to the user's stderr even when not verbose, so failures stay debuggable.

    4.5. `ExternalsHandler`

        File: `handlers/externals.rs`. Matches `externals.toml` at a pack root, parses it, and emits one `HandlerIntent::Fetch` per declared entry (`file`, `git-repo`, `archive`, `archive-file`), each carrying the entry's spec and its resolved user-side target path. The handler itself does no network work: fetching, hash verification, unpacking, and symlink creation all happen in `crate::execution::fetch`.

        Its `check_status` is coarse: the pack is deployed if `has_handler_state(pack, "external")` reports any sentinel at all, not one row per declared entry.

        Unlike the run-once handlers, a changed signature *does* re-fetch on the spot — the sentinel is keyed by the entry's upstream signature (a declared sha256, or an upstream HEAD commit for `git-repo`), and there is no user-authored code to hold back. See [./storage.lex] §5.

    4.6. `IgnoreHandler` and `SkipHandler` (filter)

        File: `handlers/filter.rs`. Both are zero-sized structs whose `to_intents` returns `Ok(vec![])` — they claim matches but emit no work. The whole effect is positional: rules tagged with handler `"ignore"` carry priority 100 and rules tagged `"skip"` carry priority 50, so during scanning either filter rule matches before any precise mapping (20 or 10) or the catchall (0) gets a chance.

        Both `check_status` impls return a placeholder `HandlerStatus { deployed: false, message: "", handler: "ignore"|"skip", file }` to satisfy the trait, but nothing reads them. Filter status is computed *directly from rule matches* in [`commands::status`]: matches under handler `"ignore"` are dropped from the output entirely, matches under `"skip"` are rendered with `Health::Skipped` (label `"skipped"`, style `skipped`). This visibility split is the entire reason for the two handlers — the matching contract is identical.

        Both are `MatchMode::Precise` and `Configuration` category. They have no datastore footprint: `dodot up` does not wipe per-pack `ignore/`/`skip/` directories because those directories never exist.

    4.7. `GateHandler` (filter)

        File: `handlers/gate.rs`. Same shape as `IgnoreHandler` / `SkipHandler`: zero-sized struct, `Filter` phase, `Configuration` category, `MatchMode::Precise`, `to_intents` returns `Ok(vec![])`. The difference is *where* matches come from: the scanner / `filter_pre_preprocess_gates` consult `crate::gates` (basename `._<label>` parsing, directory `_<label>/` segments, `[mappings.gates]` globs) against the resolved `GateTable` and `HostFacts`, and stamp `handler = "gate"` plus diagnostic options (`gate_label`, `gate_predicate`, `gate_host`) on entries whose predicate evaluated false. The handler itself never runs gate evaluation — it only exists in the registry as the dispatch target so status can render "gated out" rows uniformly with the rest of the filter family.

        Status renders gate matches via `Health::Gated { label, expected, actual }` — see `commands::status::Health::footnote_reason` for the "expected …; got …" footnote that's stamped from the `options` map. Like `ignore`/`skip`, no datastore footprint: `dodot up` does not wipe per-pack `gate/` directories because they never exist.

        Pack-level OS gating (`[pack] os = [...]`) is a *separate* mechanism that fires earlier — at pack discovery / orchestration — and produces no `RuleMatch` at all (the pack short-circuits before scanning). Inactive packs surface in `dodot status` via `PackStatusResult.inactive_packs`, not via the gate handler. See `crate::gates::pack_os_active`.

5. The Registry

    [`create_registry(fs)`] builds a `HashMap<String, Box<dyn Handler>>` keyed by handler name. The `fs` reference is needed by the run-once handlers — `install`, `homebrew`, and `nix` — for checksumming. No `CommandRunner`: building a registry spawns nothing.

    Registry construction:

        let mut registry: HashMap<String, Box<dyn Handler>> = HashMap::new();
        registry.insert(HANDLER_IGNORE.into(), Box::new(filter::IgnoreHandler));
        registry.insert(HANDLER_SKIP.into(), Box::new(filter::SkipHandler));
        registry.insert(HANDLER_GATE.into(), Box::new(gate::GateHandler));
        registry.insert(
            HANDLER_EXTERNAL.into(),
            Box::new(externals::ExternalsHandler),
        );
        registry.insert(HANDLER_SYMLINK.into(), Box::new(symlink::SymlinkHandler));
        registry.insert(HANDLER_SHELL.into(), Box::new(shell::ShellHandler));
        registry.insert(HANDLER_PATH.into(), Box::new(path::PathHandler));
        registry.insert(
            HANDLER_INSTALL.into(),
            Box::new(run_once::RunOnceHandler::new(fs, install::InstallCommand)),
        );
        registry.insert(
            HANDLER_HOMEBREW.into(),
            Box::new(run_once::RunOnceHandler::new(fs, homebrew::BrewfileCommand)),
        );
        registry.insert(
            HANDLER_NIX.into(),
            Box::new(run_once::RunOnceHandler::new(fs, nix::NixCommand)),
        );
        validate_registry(&registry);
        registry

    :: rust ::

    Well-known names are exported as constants — `HANDLER_SYMLINK`, `HANDLER_SHELL`, `HANDLER_PATH`, `HANDLER_INSTALL`, `HANDLER_HOMEBREW`, `HANDLER_NIX`, `HANDLER_IGNORE`, `HANDLER_SKIP`, `HANDLER_GATE`, `HANDLER_EXTERNAL`. Use these everywhere instead of string literals.

    The registry is hard-coded. Third-party handlers would be added via code, not user input. There is no plugin mechanism today — the trait is stable enough that writing a custom handler is straightforward, but loading them at runtime is an explicit non-goal.

6. Default Rule Mappings

    The default file-pattern → handler map lives in `config::MappingsSection`. [`config::mappings_to_rules`] converts it into the [`Rule`] set the scanner uses. The resulting rules — every handler's default patterns and priority — are generated from that code at [./../reference/handler-registry.lex] §3.

    Two handlers have no row there because they receive no default rule: `ignore` waits for a pattern the user sets under `[mappings] ignore`, and `gate` matches are minted by the scanner from `._<label>` filenames, `_<label>/` directory segments, and `[mappings.gates]` globs rather than by a rule at all. Note that the `[mappings]` key for the `external` handler is `externals`, plural.

    Priorities decide rule-evaluation order at the scanner: filter rules (`ignore` 100, `skip` 50) run before the precise rules (`external` and `install` at 20, `homebrew`, `nix`, `path`, and `shell` at 10), and the symlink catchall (0) runs last. The "first match wins" rule then routes each entry to exactly one handler — so a file that hits an `ignore` pattern is dropped before any deploying handler sees it.

    Only `skip`'s default rules are case-insensitive (so `Readme` and `readme` hit the same rule as `README`). [`Scanner::match_entries`] checks the compiled rule set once per scan and only allocates a per-entry lowercased basename when at least one rule has `case_insensitive = true` — so a user who clears `mappings.skip = []` and adds no other CI rules pays nothing; the default config exercises the CI path because the `skip` defaults populate it. See [`Rule::case_insensitive`].

    User overrides come through `[mappings]` in `.dodot.toml`. The handler list is fixed (you can't add a new handler from config), but the patterns each handler claims are fully replaceable. See [./config-system.lex] for resolution layering.

7. Datastore Layout per Handler

    Each handler's state lives under `$XDG_DATA_HOME/dodot/packs/<pack>/<handler>/`. The shape of that directory is determined by which intent the handler emits.

    Per-handler state:

        | Handler  | Intent  | Datastore contents                                                                                              |
        | symlink  | `Link`  | one symlink per source: `packs/<pack>/symlink/<filename> -> <source>`                                           |
        | shell    | `Stage` | one symlink per source: `packs/<pack>/shell/<filename> -> <source>`                                             |
        | path     | `Stage` | one symlink per source dir: `packs/<pack>/path/<dirname> -> <source>`                                           |
        | install  | `Run`   | one sentinel (+ `.snapshot`) per executed script: `packs/<pack>/install/<filename>-<checksum>`                  |
        | homebrew | `Run`   | one sentinel (+ `.snapshot`) per Brewfile: `packs/<pack>/homebrew/Brewfile-<checksum>`                          |
        | nix      | `Run`   | one sentinel (+ `.snapshot`) per manifest: `packs/<pack>/nix/packages.nix-<checksum>`                           |
        | external | `Fetch` | fetched content at `packs/<pack>/external/<name>/<filename>`, plus one sentinel per entry keyed by its signature |
        | ignore   | (none)  | nothing — the directory is never created                                                                        |
        | skip     | (none)  | nothing — the directory is never created                                                                        |
        | gate     | (none)  | nothing — the directory is never created                                                                        |

    :: table align=lll ::

    For configuration handlers, the directory IS the state — writing into it enables the handler for that pack, deleting from it disables it. There is no separate ledger.

    For code-execution handlers, the sentinel IS the "did this run with this content?" record. Sentinel content is `completed|<unix_ts>` (one line); the filename carries the content-hash key. Deleting a sentinel by hand is a supported way to force one re-run — for the run-once handlers, delete its `.snapshot` sibling with it.

    Symlink emits both halves of the double-link (data link + user-side link); shell and path emit only the data link, and the generated `dodot-init.sh` walks `packs/*/shell/` and `packs/*/path/` at shell startup. External writes the fetched copy into the datastore and then links the user-side target at it. See [./storage.lex] for the full datastore reference and [./../reference/data-layer.lex] for the conceptual model.

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

    Only `symlink` and `path` actually read this struct today. `shell`, `install`, `homebrew`, `nix`, `external`, `ignore`, `skip`, and `gate` accept it for trait uniformity but ignore the contents — their behavior is fully determined by the matched files. The narrow surface keeps handlers from coupling to config keys they don't need.

9. Adding a New Handler

    The mechanical steps:

    1. Create `handlers/<name>.rs`. A handler that runs a user-provided file once, tracked by a content-hash sentinel, implements [`RunOnceCommand`], not [`Handler`] — the registry wraps it in `RunOnceHandler<C>` and it inherits the whole run-once lifecycle (§4.4). Anything else implements [`Handler`] directly. Either way, pick a phase based on what the handler does: drop-only filtering belongs in `Filter`; fetching remote content belongs in `External`; code execution belongs in `Provision` or `Setup`; configuration belongs in `PathExport`, `ShellInit`, or `Link`.
    2. Export a name constant in `handlers/mod.rs` (`pub const HANDLER_<NAME>: &str = "<name>";`).
    3. Register it in [`create_registry`].
    4. If the handler should claim files by default, add a pattern to [`config::MappingsSection`] and emit the corresponding rule from [`config::mappings_to_rules`]. If the handler is opt-in (the user must add a rule explicitly), skip this step. A provisioning handler also needs a [`ProvisionerDescriptor`] row in `crates/dodot-lib/src/provisioners/mod.rs`: its handler name, the environment variables its command is spawned with (empty for most), the *position* of the manifest path inside the command's arguments, how the executable is located (`ExecutableLocation::Candidates` for a manager dodot probes for at fixed paths, `ExecutableLocation::Path` for the `install` case where the OS resolves an interpreter at spawn time), the manager's project page for the "not installed" message, and a version floor or `None`. A test in that module pins each declared manifest position against the argv the handler's `command_for` actually builds, so the two cannot drift.
    5. Decide whether `validate_registry` still passes. Two `Catchall` + `Exclusive` handlers will trip the `debug_assert!`.
    6. Add one [`HandlerDoc`] row to `HANDLER_CATALOG` in `handlers/catalog.rs` — the handler name, a one-line present-tense description of what a claimed file gets, and (only if the handler's matches do not come from `[mappings]` patterns) how to describe what it claims. A test holds the catalog and the registry equal in both directions: a registered handler with no row fails, and a row with no registered handler fails. Everything else in the published tables — phase, category, match mode, scope, default patterns, priority — is read from the registry and the config defaults at render time, so it cannot disagree with the shipped behavior.
    7. Run `pixi run gen-docs` to regenerate [./../reference/handler-registry.lex] and commit the result. Do not hand-edit that page: it is rendered from the registry, and the same test that renders it fails when the checked-in copy drifts. It is the roster that cannot disagree with the shipped code, so a document that needs the whole taxonomy should link to it rather than copy it.
    8. Add the handler to the pages that keep a hand-written roster anyway. They are enumerated in `CONTEXTUAL_ROSTERS` in `handlers/catalog.rs` — the README's tour, [./../user/configuration.lex], [./../user/handlers.lex], [./../user/handlers/mappings.lex], [./../user/handlers/execution-order.lex], [./../reference/handlers.lex], this document, and the two `using-dodot` skill pages. Each names the handlers in the middle of explaining something else, where a link to a separate page would cost the reader more than the duplication does, so generation does not fit. The test `contextual_rosters_name_every_handler` fails until every one of them mentions the new handler — it catches a page that forgot the handler, not a page that describes it wrongly, so read what you are editing.
    9. Write the user-facing snippet: a new file under `docs/user/handlers/`, linked from [./../user/handlers.lex]. That is the prose the generated tables cannot carry — what the handler is for, what a pack author writes, and what stays live between runs.

    :: note :: The trait surface is small (two methods carry behavior, three more are classification), and smaller still for a run-once command. A new handler is typically a few dozen lines plus tests against `TempEnvironment`.

10. Testing

    Two patterns dominate.

    Unit tests against mocks.
        Handlers that do pure intent generation (symlink target resolution, install interpreter selection) are tested with hand-built `RuleMatch` values and `HandlerConfig` overrides. No filesystem needed.

    Integration tests against `TempEnvironment`.
        Anything that reads the filesystem (symlink per-file recursion, run-once checksumming) uses [`testing::TempEnvironment`] which builds a real temp directory with isolated home and datastore. The pattern is fluent: `.pack("vim").file("vimrc", "x").done().build()`. See [./types-and-structure.lex] §6 for details.

    The handler trait itself has compile-time object-safety checks in `mod.rs` (`assert_object_safe`, `assert_boxable`) plus tests asserting the registry has exactly one exclusive catchall, that phase ordering matches declaration order, and that each built-in handler is in the phase it is supposed to be in. `RunOnceCommand` gets the same object-safety check in `run_once.rs`.

    A third pattern guards the documentation. The tests in `handlers/catalog.rs` hold `HANDLER_CATALOG` and the registry equal in both directions, and re-render [./../reference/handler-registry.lex] to compare against the checked-in file — so a handler that ships without a catalog row, or with a stale generated page, fails `pixi run test`. A third test reads the pages in `CONTEXTUAL_ROSTERS` and fails when one of them does not mention a registered handler, which is the drift that actually happened when `nix`, `external`, and `gate` landed. It checks for a mention and nothing more: it cannot tell that a page put a handler in the wrong phase.
