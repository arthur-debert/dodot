Handlers

    A handler is the thing that decides what to do with a file once dodot has decided to process it. Each handler has exactly one job: link configs, source shell scripts, add directories to `$PATH`, run install scripts once, or install Brewfiles. This document describes the handlers dodot ships, the rules for how matches flow to them, and the distinction between handlers that always run and handlers that run once.

    :: note :: See [./terms-and-concepts.lex] for terminology used throughout.

1. The Built-in Handlers

    dodot ships with five handlers, covering the overwhelming majority of what dotfile repositories need.

    1.1. symlink

        Creates a symlink from a deployed location (typically `~/` or `~/.config/`) back to a file or directory in your pack. This is the default for any file that no other handler claims — anything that looks like plain configuration flows through here.

        Path resolution is smart: every pack-root entry — file or directory — defaults to `$XDG_CONFIG_HOME/<pack>/<name>` (so `nvim/init.lua` → `~/.config/nvim/init.lua`, `warp/themes/` → `~/.config/warp/themes/`). A small list of exceptions force `$HOME` placement regardless of XDG (`ssh`, `bashrc`, `zshrc`, etc.); the per-file `home.X` prefix and per-subtree `_home/` directory route opt-in single files or whole subtrees to `$HOME/.X`. For the full path rules, see [./symlink-paths.lex].

    1.2. shell

        Arranges for shell scripts to be sourced at login. Matches `aliases.sh`, `profile.sh`, and `login.sh` by default; add more patterns via `[mappings] shell` in `.dodot.toml`. The mechanism is a single `eval "$(dodot init-sh)"` line in your shell rc; the generated init script walks the datastore and emits `source` lines for every matched shell file.

    1.3. path

        Exposes a directory on your `$PATH`. The conventional match is a `bin/` directory inside a pack; its contents become directly executable from any shell. Like shell, this rides on the dodot init script — the datastore records which directories should be on PATH, and the init script prepends them.

    1.4. install

        Runs an arbitrary shell script once, tracked by a sentinel file so it doesn't re-run on every deploy. Matches `install.sh` by convention. Use this for machine-specific setup that isn't covered by the other handlers: installing language toolchains, configuring window managers, creating directories, setting system defaults.

    1.5. homebrew

        Runs `brew bundle` against a `Brewfile`, once per content-hash. macOS-only in practice. Functionally a specialization of install, but more ergonomic for its common case.

2. Matching Model

    Handlers are classified along two axes that together decide how matches flow.

    Match mode:
        _Precise_ handlers claim specific names or patterns: `install.sh`, `Brewfile`, `bin/`, `*.sh`. _Catchall_ handlers claim anything precise handlers didn't touch. Precise handlers run first and consume their matches; the catchall sees only what's left.

        At most one handler may be catchall in a given pack. Today that role is played by `symlink`. This isn't a rule of the matching system so much as a practical consequence: two catchalls would race for every unclaimed file.

    Scope:
        _Exclusive_ matches are consumed on first claim — no other handler sees that entry. _Shared_ matches remain available after a claim, so multiple handlers can act on the same entry. All current handlers are exclusive. Shared scope is reserved for future observer-style handlers (an audit handler, a stats handler) that watch without deploying.

    The scanner that produces matches works only at the pack's top level — it does not recurse. A handler that receives a directory entry decides how to treat its contents. The path handler stages the whole directory into `$PATH`; the symlink handler creates one symlink for the directory as a whole. If you want a nested path handled independently, you declare it explicitly in `.dodot.toml` (via `[symlink.targets]` or by naming a file inside it in `[symlink] protected_paths`).

3. Configuration vs Code Execution

    Handlers fall into two categories that behave differently at deploy time.

    3.1. Configuration handlers

        symlink, shell, and path. Their operations are idempotent filesystem work: create a link, stage a file. Running them a second time produces the same result as running them once; no special tracking is required. `dodot up` always runs them in full.

    3.2. Code execution handlers

        install and homebrew. Their operations run user-authored shell commands. These are assumed _not_ to be idempotent in general — `install.sh` might install packages, write files, mutate the system — and re-running them on every `dodot up` would be slow, surprising, or both. Even Brewfile processing, though nominally idempotent, can take many seconds per pack.

        dodot solves this with sentinels. When a code-execution handler runs, it writes a small marker file to the datastore keyed by pack, handler, and a content hash of the command. On subsequent deploys, the presence of that sentinel causes the handler to skip. To override:

        - `--no-provision` skips code-execution handlers entirely for this run. Configuration handlers still run.
        - `--provision-rerun` forces code-execution handlers to run even when sentinels exist. Use after changing an install script, or to re-run `brew bundle` after adding a formula.

        When the content of a code-execution input changes (you edited `install.sh`, or the rendered output of `install.sh.tmpl` changed), the sentinel's content hash no longer matches, and the handler re-runs automatically. You only need `--provision-rerun` when you want to re-run without an input change.

4. Quick Reference

    Handler summary:

        | Handler  | Category       | Default claims                                  | Effect                              |
        | symlink  | Configuration  | Anything else (catchall)                        | Link to `~` or `~/.config/`         |
        | shell    | Configuration  | `aliases.sh`, `profile.sh`, `login.sh`          | Sourced at shell login              |
        | path     | Configuration  | `bin/`                                          | Prepended to `$PATH`                |
        | install  | Code Execution | `install.sh`                                    | Run once per content hash           |
        | homebrew | Code Execution | `Brewfile`                                      | `brew bundle` once per content hash |

    :: table align=llll ::

5. Why Handlers Look the Way They Do

    A few design decisions worth naming.

    Handlers do not touch the filesystem.
        They read matches and produce intents. The actual work of creating links, running commands, and writing sentinels happens in layers below (executor, datastore). This keeps handlers small — each is a few dozen lines — and trivially testable without a real filesystem.

    Handlers are replaceable but not pluggable.
        The trait they implement is stable enough that writing a custom handler is not hard, but dodot does not load third-party handlers at runtime. The built-in set is deliberately small; we'd rather add handlers carefully than ship a plugin system we have to maintain.

    The catchall is always symlink.
        This is a convention rather than a hard rule of the code, but it's the only combination that preserves the "just name it sensibly" promise. If no precise handler matched, we know the user wanted the file deployed somewhere sensible, and a link is the right default.
