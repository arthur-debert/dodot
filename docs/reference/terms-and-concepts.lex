Terms and Concepts

    dodot's shared vocabulary. Other reference docs refer back here rather than redefine these terms. If you see a term used elsewhere that isn't defined here, that's a documentation bug — please open an issue.

1. Foundational

    Dotfiles root:
        The top-level directory containing your dotfile packs. dodot selects it from `$DOTFILES_ROOT`, then the enclosing git repository's top level, then the current directory. A present but invalid `$DOTFILES_ROOT` is an error; there is no `~/dotfiles` fallback. The root IS the source of truth for pack content; dodot keeps its own bookkeeping in the data directory, not alongside your configs.

    Pack:
        A directory under the dotfiles root whose contents belong together — `vim/`, `git/`, `work/`. Packs are the unit dodot turns on and off. The organizing criterion is yours (by application, by role, by machine); dodot treats every top-level directory as a pack unless it contains a `.dodotignore` file.

    Handler:
        A named transformer that decides what to do with a file during deployment. Each handler has one job: `symlink` creates links, `shell` sources scripts at login, `path` adds directories to `$PATH`, `external` fetches upstream content, `install` runs setup scripts once, `homebrew` and `nix` install packages from a `Brewfile` and a `packages.nix`, and `ignore`, `skip`, and `gate` drop a file from processing. Ten in all; the generated roster is at [./handler-registry.lex], and [./handlers.lex] says what each is for.

    Rule:
        A pattern-to-handler mapping with a priority. Rules are how dodot decides which handler claims which file — e.g. `install.sh` → install, `aliases.sh` → shell, `*` → symlink. Rules are declarative; matching is in priority order, first match wins.

2. Handler Taxonomy

    Handlers are classified along two independent axes, plus a third categorization by side-effect risk.

    Match mode:
        _Precise_ handlers claim whitelisted names or patterns (`Brewfile`, `install.sh`, `bin/`). _Catchall_ handlers claim anything precise handlers didn't. Only one catchall may exist in a pack; today that role is played by `symlink`.

    Scope:
        _Exclusive_ handlers consume their match — no other handler sees that entry. _Shared_ handlers leave the match available (reserved for future observer-style handlers). All current handlers are exclusive.

    Configuration vs Code Execution:
        _Configuration_ handlers (symlink, shell, path, and the three filter handlers ignore, skip, and gate) produce idempotent file operations — or, for the filters, no operation at all — and always run. _Code Execution_ handlers (external, install, homebrew, nix) run commands or fetch remote content, and a sentinel records what already ran so the work is not repeated on every `dodot up`. Also called _provisioning_ handlers. The category is derived from the handler's phase, not declared per handler.

3. Processing Pipeline

    Intent:
        A handler's declaration of what it wants to happen, independent of how. Three shapes: `Link` (connect a source to a target), `Stage` (make a file reachable via the datastore), `Run` (execute a command once, gated by a sentinel). Handlers produce intents; they never touch the filesystem directly.

    Operation:
        The concrete, filesystem-level work generated from an intent. Four kinds: `CreateDataLink`, `CreateUserLink`, `RunCommand`, `CheckSentinel`. Operations are what actually runs — or, in `--dry-run`, what is reported.

    DataStore:
        The abstraction that executes operations. Currently backed by the filesystem, but the interface deliberately admits other backends. Code outside the datastore layer is oblivious to where state lives or how it is stored.

    Datastore directory:
        The on-disk location where dodot keeps its links and sentinels, at `$XDG_DATA_HOME/dodot/` (default `~/.local/share/dodot/`). This is the only place dodot writes outside the dotfiles root.

4. State and Storage

    Double-link:
        dodot's data model. Instead of linking a deployed path (`~/.gitconfig`) directly to a source file (`~/dotfiles/git/gitconfig`), dodot inserts an intermediate link in the datastore — `~/.gitconfig` → `datastore/…/gitconfig` → `~/dotfiles/git/gitconfig`. The intermediate layer is what makes deployment status queryable without a database. See [./data-layer.lex].

    Sentinel:
        A marker file recording that a code-execution handler has completed. Prevents re-running install scripts or re-installing Brewfiles on every deploy. Sentinels are keyed by pack, handler, and a hash of the content that ran, so dodot can tell a file that never ran from one that ran with different content. A changed hash does not re-run anything on its own: `dodot up` reports the file as an older version and holds until you pass `--provision-rerun`.

    Shell integration:
        A single line in your shell rc that sources shell scripts from the datastore and adds PATH directories. `dodot install --write` owns that line (a marked block sourcing the generated script by path); `eval "$(dodot init-sh)"` is the equivalent hand-wired form. The init script itself is generated by dodot; there is no logic duplicated in shell code.

    Hookup:
        The rc wiring — the line in your shell startup file. A hookup can exist and still never run.

    Activation:
        A shell actually sourcing the init script. Distinct from *hookup*, and distinct from deployment: `dodot up` proves packs are in the datastore, activation is whether any shell loads them. The two layers fail independently, which is why dodot reports on both.

    Generation:
        One regeneration of the init script, stamped as the unix second it was written. The script exports its generation into the environment, so any dodot command can tell whether the calling shell loaded the current script, an older one, or none.

    Evidence:
        What the init script leaves behind to prove it ran — the generation stamp in the environment, and a heartbeat file in the data directory. Cheap to read, and the basis for everything `status` reports about activation.

    Probe:
        Starting the user's shell to measure activation when evidence is inconclusive. `dodot up` probes only when the cheap signals can't answer; `dodot install --write` probes after writing. `dodot status` never does. See [./../user/shell-integration.lex] §5.

5. Preprocessing

    Preprocessor:
        A pipeline phase that transforms a source file before handlers see it. The rendered output lives in the datastore; downstream handlers deploy it as if it had always been a regular file. Preprocessing composes with all handlers — a rendered `aliases.sh.tmpl` still flows through the shell handler. See [./pre-processors.lex].

    Transform type:
        The shape of a preprocessor's reverse path, which determines how divergence is handled. _Generative_ (one-way): source produces the deployed artifact; reverse is best-effort via heuristics (templates). _Representational_ (two-way): source and deployed are equivalent representations; reverse is exact (plists, XML ↔ binary). _Opaque_ (write-only): source is decrypted or decoded on deploy; no reverse path exists (encrypted secrets).

6. User Conventions

    `dot.` prefix:
        A filename prefix dodot strips on deploy: `dot.bashrc` in a pack becomes `~/.bashrc`. Keeps dotfiles visible in your editor and `ls` output without losing the dot-file convention at the deployed location.

    `_home/`, `_xdg/`, `_app/`, and `_lib/` directories:
        Explicit routing overrides. Files under `<pack>/_home/` deploy to `$HOME`; under `<pack>/_xdg/` to `$XDG_CONFIG_HOME`; under `<pack>/_app/` to `app_support_dir` (macOS `~/Library/Application Support`, Linux `$XDG_CONFIG_HOME`); under `<pack>/_lib/` to `$HOME/Library/` (macOS only — non-macOS platforms emit a soft warning and skip). All four prefixes skip pack-namespacing. See [./symlink-paths.lex].

    App-support root:
        The third filesystem coordinate dodot routes against, alongside `$HOME` and `$XDG_CONFIG_HOME`. On macOS it points at `~/Library/Application Support` (canonical home for GUI app config). On Linux it collapses onto `$XDG_CONFIG_HOME` so the same pack tree works on both. Toggleable on macOS via `[symlink] app_uses_library = false`.

    App alias:
        A pack-name → app-folder-name rewrite declared via `[symlink.app_aliases]`. Lets a pack named `vscode` deploy to `<app_support_dir>/Code/...` so the pack name stays lowercase-ergonomic while the deployed folder name matches what the GUI app actually reads. Modifies the resolver's default rule only — explicit prefixes still win.

    `force_app`:
        Curated list of GUI-app folder names whose first path segment routes to `<app_support_dir>/<name>/<rest>` without requiring a `_app/` prefix. Mirror of `force_home` for the third coordinate. Case-sensitive, capped at 100 entries; ships with a small seed (Code, Cursor, Zed, Emacs).

    `.dodotignore` (pack-ignore):
        A marker file that tells dodot to skip a pack entirely — the "pack-ignore" mechanism. Pure file-presence check; the file's contents are never read. Useful for directories that live in the dotfiles root but aren't meant to be deployed (scratch, notes, README-only packs). Distinct from the intra-pack `[mappings] ignore`/`skip` keys, which drop individual files inside a known pack.

    `.dodot.toml`:
        Per-pack or root-level configuration. Overrides defaults for mappings, symlink targets, preprocessor settings, and more. Root config applies to all packs; pack config overrides root for that pack.

7. Conditional Running

    Gate:
        A predicate that decides whether dodot deploys an entry on the current host. Five gate surfaces sit at three granularities: filename suffix `._<label>` (one file), directory segment `_<label>/` (one subtree), `[pack] os` (whole pack), `[mappings.gates]` glob (legacy escape hatch), and `dodot adopt --only-os` (round-trip). Gates that fail surface in `dodot status` as `gated out (<label>)`; gates that pass strip their suffix and proceed through normal handler dispatch.

    Gate label:
        An opaque token (e.g. `darwin`, `linux`, `arm-mac`, `laptop`) that names a host predicate. Labels resolve through a `GateTable` to a set of `(dimension, value)` equality checks AND-ed together. Built-ins ship for OS and arch; user-defined labels live under `[gates]` in `.dodot.toml`. Label names must match `[A-Za-z0-9_-]+` and must not collide with routing-prefix tokens (`home`/`xdg`/`app`/`lib`).

    Gate table:
        The resolved label → predicate map for a pack. Built-in seed (compiled defaults) layered with `[gates]` entries from root and pack `.dodot.toml`. User entries with the same name as a built-in replace the built-in's predicate wholesale (no per-dimension merging).

    Gate failure:
        A `RuleMatch` with `handler = "gate"` produced when an entry's predicate evaluates false on the current host. Carries diagnostic options (`gate_label`, `gate_predicate`, `gate_host`) that `dodot status` reads to render the "expected …; got …" footnote on the row.

    Host facts:
        Snapshot of the host's gate-relevant values (`os`, `arch`, `hostname`, `username`) — the `dodot.*` namespace's runtime view. Detected once per `ExecutionContext` and reused across all per-pack scans so `hostname(1)` doesn't fire repeatedly. The gate machinery shares its `detect_hostname` / `detect_username` with the template preprocessor so `dodot.hostname` agrees between the two paths.
