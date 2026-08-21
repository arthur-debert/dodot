Storage

    This document covers the `DataStore` trait API and the on-disk layout of the datastore directory. It is the contributor-facing complement to [./../reference/data-layer.lex], which covers the conceptual model.

    :: note :: See [./../reference/terms-and-concepts.lex] for terminology used throughout.

1. Filesystem Layout

    The datastore lives at `$XDG_DATA_HOME/dodot/` (default `~/.local/share/dodot/`). Its structure is organized by pack, then by handler.

    Datastore layout:

        ~/.local/share/dodot/
        +-- packs/
        |   +-- <pack>/
        |       +-- symlink/              # intermediate symlinks
        |       |   +-- <name> -> <source>
        |       +-- shell/                # staged shell scripts
        |       |   +-- <name> -> <source>
        |       +-- path/                 # staged path directories
        |       |   +-- <name> -> <source>
        |       +-- homebrew/             # sentinels (e.g. "Brewfile-a1b2c3d4e5f6a7b8")
        |       |   +-- <name>-<checksum>.snapshot   # bytes that ran
        |       +-- nix/                  # sentinels (e.g. "packages.nix-a1b2c3d4e5f6a7b8")
        |       |   +-- <name>-<checksum>.snapshot   # bytes that ran
        |       +-- install/              # sentinels (e.g. "install.sh-a1b2c3d4e5f6a7b8")
        |       |   +-- <name>-<checksum>.snapshot   # bytes that ran
        |       +-- external/             # one sentinel per fetched entry
        |       |   +-- <entry>/          # the fetched content itself
        |       +-- preprocessed/         # preprocessor output (rendered files)
        |           +-- <stripped-name>
        +-- probes/
        |   +-- hookup/
        |       +-- heartbeat             # last shell activation (a generation)
        +-- shell/
            +-- dodot-init.sh             # generated shell integration script

    :: text ::

    The three filter handlers — `gate`, `ignore`, and `skip` — never appear here. They claim matches and emit no work, so their directories are never created.

    Two invariants hold:

    - Everything dodot writes outside the dotfiles root lives under this tree. If it isn't here, dodot didn't put it there.
    - Each `packs/<pack>/<handler>/` directory IS the handler's state for that pack. Writing to it enables; deleting from it disables. No separate ledger.

2. The DataStore Trait

    Defined in `datastore::DataStore`. Fourteen methods, grouped by purpose.

    2.1. Link Creation

        `create_data_link(pack, handler, source_file) -> Result<PathBuf>`:
            Creates the intermediate symlink under `packs/<pack>/<handler>/<filename>` pointing at `source_file`. Returns the absolute datastore path. Idempotent.

        `create_user_link(datastore_path, user_path) -> Result<()>`:
            Creates the user-visible symlink `user_path -> datastore_path`. Creates parent directories as needed. Idempotent.

        Together these form the double-link. Handlers that want a full deployment call both through a `Link` intent; handlers that only need the file in the datastore (shell, path) call just the first through a `Stage` intent.

    2.2. Code Execution

        Four handlers are `CodeExecution` and write sentinels: `install`, `homebrew`, and `nix` — the run-once handlers, which spawn a command through `run_and_record` — and `external`, which fetches rather than spawns and keys its sentinel on an upstream signature. `dodot up` never wipes their state before re-applying, which is exactly what keeps a script from running on every deploy and an external from being re-downloaded on every deploy.

        `run_and_record(pack, handler, command, sentinel, force) -> Result<()>`:
            Runs a command, records a sentinel on success. If the sentinel already exists and `force` is false, the command is skipped. The sentinel file stores `completed|{timestamp}`. The `force` flag on this method is how `--provision-rerun` is implemented; it does not appear on `HandlerIntent::Run`.

            `command` is a `CommandSpec` — executable, arguments, and the environment to spawn with. See section 4.

            On success it also writes a `<sentinel>.snapshot` sibling holding the bytes of the manifest that ran, and it names that manifest in the run's progress header. Which argument carries the manifest is declared per handler in `provisioners::PROVISIONERS`; a command with no descriptor there runs normally but names no file, so it gets no snapshot and a header naming the executable instead.

            Edge case: if the command succeeds but the sentinel write fails, a subsequent call re-runs the command. This is by design — re-running is safer than falsely marking as complete. Install scripts are expected to be idempotent for this reason.

        `did_run(pack, handler, filename, current_hash) -> Result<DidRunStatus>`:
            The three-way "has this file run, and is what ran current?" lookup the run-once handlers (`install`, `homebrew`, `nix`) plan against. Lists the handler directory for sentinels named `<filename>-<16 hex chars>`, whatever the hash, and answers `NeverRan` (none), `RanCurrent` (one matches `current_hash`), or `RanDifferent` (only other hashes) — the last carrying the prior hash and, when the sibling is on disk, the snapshot of the file as it was at that run.

            Multiple non-matching sentinels tie-break on the most recently completed run, read from each sentinel's `completed|<unix-ts>` payload; unparseable payloads sort last, and a timestamp tie breaks on the sentinel filename so the answer is deterministic.

        `has_sentinel(pack, handler, sentinel) -> Result<bool>`:
            Tests for sentinel existence. This is the external handler's check — it asks about one exact signature rather than about a family of hashes.

        `sentinel_path(pack, handler, sentinel) -> PathBuf`:
            Returns where a sentinel would be stored; useful for inspection and testing.

    2.3. State Management

        `remove_state(pack, handler) -> Result<()>`:
            Deletes the `packs/<pack>/<handler>/` directory and everything in it. This is what `dodot down` calls per handler.

        `has_handler_state(pack, handler) -> Result<bool>`:
            Tests whether a handler has any state for a pack (any files in its directory).

        `list_packs() -> Result<Vec<String>>`:
            Lists the pack directory names that have a subtree under `packs/`. These are on-disk datastore keys (`010-nvim`), not display names. Because it walks the datastore rather than the dotfiles root, it is what finds state whose pack the repository has since deleted.

        `list_pack_handlers(pack) -> Result<Vec<String>>`:
            Lists handler names that have state for a pack. Used by `down` to discover what needs removal without re-running rule matching.

        `list_handler_sentinels(pack, handler) -> Result<Vec<String>>`:
            Lists sentinel filenames for a pack/handler. Used by status reporting.

    2.4. Preprocessor Output

        `write_rendered_file(pack, handler, filename, content) -> Result<PathBuf>`:
            Writes a regular (non-symlink) file under `packs/<pack>/<handler>/<filename>`. Used by preprocessors that produce content rather than pointing at existing files, and by the external handler for fetched content and its sentinels. Returns the absolute path. Idempotent — it overwrites.

        `write_rendered_file_with_mode(pack, handler, filename, content, mode) -> Result<PathBuf>`:
            The same, but applies `mode` at file-creation time so the bytes never sit on disk under a more permissive mode — what whole-file `age` / `gpg` plaintext needs. The default implementation writes then chmods, which is briefly permissive; real implementations override it with the atomic `Fs::write_file_with_mode` path.

        `write_rendered_dir(pack, handler, relative) -> Result<PathBuf>`:
            Creates an empty directory inside the datastore (mkdir -p semantics). Used for preprocessors that need to materialize directory entries, such as the unarchive preprocessor.

        Both enforce path safety: `filename` and `relative` are validated by callers to reject absolute paths and `..` components; the implementation enforces the same as defense in depth.

3. `FilesystemDataStore`

    The production implementation of `DataStore`, in `datastore::filesystem::FilesystemDataStore`. Takes a root path (`$XDG_DATA_HOME/dodot/`) and a `CommandRunner` for `run_and_record`. Everything else is `std::fs` operations.

    The implementation is small enough to read end-to-end (a few hundred lines). Edge-case handling — broken symlinks, partial state, race conditions between processes — is concentrated here; the rest of the codebase treats the trait as correct.

4. `CommandRunner`

    Separate trait, also in `datastore`. Abstracts command execution so tests can inject a mock.

    CommandRunner:

        pub struct CommandSpec<'a> {
            pub executable:  &'a str,
            pub arguments:   &'a [String],
            pub environment: &'a [(String, String)],
        }

        pub trait CommandRunner: Send + Sync {
            fn run(&self, command: CommandSpec<'_>) -> Result<CommandOutput>;
        }

    :: rust ::

    Production uses `ShellCommandRunner`, which spawns a real subprocess. Tests typically use a mock that records calls and returns scripted outputs.

    A spec's `environment` is *layered* onto the environment dodot is running with — `ShellCommandRunner` calls `Command::envs` and never `env_clear`, so the child keeps the user's `PATH`, `HOME`, and everything else, with the spec's rows overriding or adding on top. `CommandSpec::new(executable, arguments)` carries no rows and is what every caller outside provisioning uses; `CommandSpec::with_environment` is how a provisioning handler's descriptor rows reach the process.

5. Sentinel Format

    A run-once sentinel is a small file named `<basename>-<checksum>`, where `<basename>` is the matched file's own name (`install.sh`, `Brewfile`, `packages.nix`) and `<checksum>` is the first 8 bytes of `SHA-256(content)` hex-encoded — 16 hex characters. Example filename: `install.sh-a1b2c3d4e5f6a7b8`. SHA-256 is the only hash dodot computes here; the truncation is for readable directory listings, not for security.

    The content hashed is the content that would run: for a preprocessor-rendered file that is the rendered bytes held in memory, not whatever sits on disk, which is what lets `dodot status` and `up --dry-run` compute the right sentinel for a templated `install.sh` without materializing it.

    The file content is `completed|{timestamp}` — one line, the literal string `completed`, a pipe, and a Unix epoch timestamp. A successful run also leaves a `<sentinel>.snapshot` sibling holding the bytes that ran; that is what the `(N lines added, M removed)` summary and `dodot status --diff` read. Sentinels written before snapshots existed have no sibling, and their rows say `older version (no diff data)`.

    The external handler names its sentinels differently, because what it keys on is an upstream signature rather than a local file's content: `<entry>-<sig>` for a `file` entry (the first 16 hex chars of the sha256 the user declared in `externals.toml`), `<entry>-git-<sha>` for a `git-repo` (the upstream HEAD it deployed), and `<entry>-archive-<sha>` — plus a hash of the member path for `archive-file` — for archives.

    Because the checksum is part of the sentinel name, any change to the input content produces a new sentinel name — which is how dodot detects that an `install.sh` has been edited or that a preprocessor produced different output on a new machine. For the run-once handlers (install, homebrew, nix) detection is not application: [`DataStore::did_run`] reports the mismatch as `RanDifferent`, the executor skips the command with a "ran older version" notice, and applying the edit takes `dodot up --provision-rerun`. The external handler is the exception — a changed signature re-fetches on the spot, since there is no user-authored code to hold back.

    Sentinels are cheap to inspect, cheap to delete, and contain no information you can't reproduce. Deleting one by hand — together with its `.snapshot` sibling — is a supported way to force a re-run of its handler without using `--provision-rerun`.

6. Shell Integration Script

    `dodot-init.sh` is generated by `dodot_lib::shell::generate_init_script`, which walks the datastore and emits:

    - one `source '<path>'` line per file in `packs/*/shell/`
    - one `export PATH=...` line that prepends every directory in `packs/*/path/` to `$PATH`

    No logic in the shell script beyond that. Regenerated on every `dodot up` and `dodot down` so it always reflects current state.

    The script is what users source via `eval "$(dodot init-sh)"`; `init-sh` simply prints the generated contents to stdout.

    6.1. The Generation Contract

        Every generated script — profiled or not, empty datastore or not — opens with two lines of *activation evidence*:

        Evidence block:

            export DODOT_INIT_GEN=<generation>
            echo <generation> > '<data_dir>/probes/hookup/heartbeat' 2>/dev/null || :

        :: shell ::

        A *generation* is the unix second the script was written at. `write_init_script` stamps `shell::activation::current_generation()`; `generate_init_script` takes it as an argument, so tests pin it and the emitted script stays deterministic. `dodot init-sh` stamps the current second too — that shell is activating right now.

        Three readers consume it, all in `shell::activation`:

        - `read_env_stamp` — `DODOT_INIT_GEN` from the calling shell's environment. Present means *this* shell sourced init; the value says which generation.
        - `read_heartbeat` — the marker's contents. Present means *some* shell has activated on this machine; absent means none ever has.
        - `read_script_generation` — the `export` line of the script on disk, i.e. the generation a shell started right now would load.

        Two invariants keep the evidence honest:

        - *The write side owns the directory.* `write_init_script` creates `probes/hookup/` so the emitted redirect never has to. A `mkdir -p` in the script would cost a process on every shell start.
        - *One export, one redirect.* No command substitution, no `dodot` invocation, nothing that forks. The heartbeat is a truncating write of static content, which is also what makes concurrent shell startups safe: last writer wins, and every writer writes the same bytes.

        `dodot up` and `dodot status` evaluate the two signals against a reference generation into one activation state (healthy / stale shell / never activated). `status` compares against the script on disk; `up` captures the generation *before* it regenerates, because comparing against the script it just wrote would call every shell stale on every deploy. See `docs/proposals/shipped/shell-hookup.lex` §2 and §5.

7. Path Safety

    Methods that take a `filename` or `relative` argument (`write_rendered_file`, `write_rendered_dir`) are the only places untrusted path components cross the datastore boundary. Both enforce:

    - No absolute paths
    - No `..` components
    - No components starting with `/`

    The preprocessing pipeline validates inputs before calling, and the datastore layer validates again. This is intentional belt-and-suspenders — preprocessor bugs shouldn't be able to write outside the datastore.

8. Testing

    `FilesystemDataStore` is exercised directly by integration tests via `testing::TempEnvironment`, which builds one over a real temp directory. Unit tests can use `MockDataStore` (defined in `testing`) that records calls without touching a filesystem.

    The trait surface is small enough that writing a custom `DataStore` for a specialized backend — a remote filesystem, an in-memory store for fuzzing — is on the order of a single file.
