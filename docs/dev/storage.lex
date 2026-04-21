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
        |       +-- install/              # sentinels (e.g. "install.sh-a1b2c3d4e5f6a7b8")
        |       +-- preprocessed/         # preprocessor output (rendered files)
        |           +-- <stripped-name>
        +-- shell/
            +-- dodot-init.sh             # generated shell integration script

    :: text ::

    Two invariants hold:

    - Everything dodot writes outside the dotfiles root lives under this tree. If it isn't here, dodot didn't put it there.
    - Each `packs/<pack>/<handler>/` directory IS the handler's state for that pack. Writing to it enables; deleting from it disables. No separate ledger.

2. The DataStore Trait

    Defined in `datastore::DataStore`. Eleven methods, grouped by purpose.

    2.1. Link Creation

        `create_data_link(pack, handler, source_file) -> Result<PathBuf>`:
            Creates the intermediate symlink under `packs/<pack>/<handler>/<filename>` pointing at `source_file`. Returns the absolute datastore path. Idempotent.

        `create_user_link(datastore_path, user_path) -> Result<()>`:
            Creates the user-visible symlink `user_path -> datastore_path`. Creates parent directories as needed. Idempotent.

        Together these form the double-link. Handlers that want a full deployment call both through a `Link` intent; handlers that only need the file in the datastore (shell, path) call just the first through a `Stage` intent.

    2.2. Code Execution

        `run_and_record(pack, handler, executable, arguments, sentinel, force) -> Result<()>`:
            Runs a command, records a sentinel on success. If the sentinel already exists and `force` is false, the command is skipped. The sentinel file stores `completed|{timestamp}`. The `force` flag on this method is how `--provision-rerun` is implemented; it does not appear on `HandlerIntent::Run`.

            Edge case: if the command succeeds but the sentinel write fails, a subsequent call re-runs the command. This is by design — re-running is safer than falsely marking as complete. Install scripts are expected to be idempotent for this reason.

        `has_sentinel(pack, handler, sentinel) -> Result<bool>`:
            Tests for sentinel existence.

        `sentinel_path(pack, handler, sentinel) -> PathBuf`:
            Returns where a sentinel would be stored; useful for inspection and testing.

    2.3. State Management

        `remove_state(pack, handler) -> Result<()>`:
            Deletes the `packs/<pack>/<handler>/` directory and everything in it. This is what `dodot down` calls per handler.

        `has_handler_state(pack, handler) -> Result<bool>`:
            Tests whether a handler has any state for a pack (any files in its directory).

        `list_pack_handlers(pack) -> Result<Vec<String>>`:
            Lists handler names that have state for a pack. Used by `down` to discover what needs removal without re-running rule matching.

        `list_handler_sentinels(pack, handler) -> Result<Vec<String>>`:
            Lists sentinel filenames for a pack/handler. Used by status reporting.

    2.4. Preprocessor Output

        `write_rendered_file(pack, handler, filename, content) -> Result<PathBuf>`:
            Writes a regular (non-symlink) file under `packs/<pack>/<handler>/<filename>`. Used by preprocessors that produce content rather than pointing at existing files. Returns the absolute path.

        `write_rendered_dir(pack, handler, relative) -> Result<PathBuf>`:
            Creates an empty directory inside the datastore (mkdir -p semantics). Used for preprocessors that need to materialize directory entries, such as the unarchive preprocessor.

        Both enforce path safety: `filename` and `relative` are validated by callers to reject absolute paths and `..` components; the implementation enforces the same as defense in depth.

3. `FilesystemDataStore`

    The production implementation of `DataStore`, in `datastore::filesystem::FilesystemDataStore`. Takes a root path (`$XDG_DATA_HOME/dodot/`) and a `CommandRunner` for `run_and_record`. Everything else is `std::fs` operations.

    The implementation is small enough to read end-to-end (a few hundred lines). Edge-case handling — broken symlinks, partial state, race conditions between processes — is concentrated here; the rest of the codebase treats the trait as correct.

4. `CommandRunner`

    Separate trait, also in `datastore`. Abstracts command execution so tests can inject a mock.

    CommandRunner:

        pub trait CommandRunner: Send + Sync {
            fn run(&self, executable: &str, arguments: &[String]) -> Result<CommandOutput>;
        }

    :: rust ::

    Production uses `ShellCommandRunner`, which spawns a real subprocess. Tests typically use a mock that records calls and returns scripted outputs.

5. Sentinel Format

    Sentinels are small files named `<source>-<checksum>`, where `<source>` is the originating filename (e.g., `install.sh`, `Brewfile`) and `<checksum>` is the first 16 hex characters of a SHA-256 hash of the input content. Example filename: `install.sh-a1b2c3d4e5f6a7b8`.

    The file content is `completed|{timestamp}` — one line, the literal string `completed`, a pipe, and a Unix epoch timestamp.

    Because the checksum is part of the sentinel name, any change to the input content produces a new sentinel name, which causes the handler to re-run automatically. This is how dodot detects that an `install.sh` has been edited or that a preprocessor produced different output on a new machine.

    Sentinels are cheap to inspect, cheap to delete, and contain no information you can't reproduce. Deleting one by hand is a supported way to force a re-run of its handler without using `--provision-rerun`.

6. Shell Integration Script

    `dodot-init.sh` is generated by `dodot_lib::shell::generate_init_script`, which walks the datastore and emits:

    - one `source '<path>'` line per file in `packs/*/shell/`
    - one `export PATH=...` line that prepends every directory in `packs/*/path/` to `$PATH`

    No logic in the shell script beyond that. Regenerated on every `dodot up` and `dodot down` so it always reflects current state.

    The script is what users source via `eval "$(dodot init-sh)"`; `init-sh` simply prints the generated contents to stdout.

7. Path Safety

    Methods that take a `filename` or `relative` argument (`write_rendered_file`, `write_rendered_dir`) are the only places untrusted path components cross the datastore boundary. Both enforce:

    - No absolute paths
    - No `..` components
    - No components starting with `/`

    The preprocessing pipeline validates inputs before calling, and the datastore layer validates again. This is intentional belt-and-suspenders — preprocessor bugs shouldn't be able to write outside the datastore.

8. Testing

    `FilesystemDataStore` is exercised directly by integration tests via `testing::TempEnvironment`, which builds one over a real temp directory. Unit tests can use `MockDataStore` (defined in `testing`) that records calls without touching a filesystem.

    The trait surface is small enough that writing a custom `DataStore` for a specialized backend — a remote filesystem, an in-memory store for fuzzing — is on the order of a single file.
