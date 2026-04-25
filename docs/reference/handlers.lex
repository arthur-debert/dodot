Handlers

    A handler is the thing that decides what to do with a file once dodot has decided to process it. Each handler has exactly one job: link configs, source shell scripts, add directories to `$PATH`, run install scripts once, or install Brewfiles. This document describes the handlers dodot ships, the rules for how matches flow to them, and the distinction between handlers that always run and handlers that run once.

    :: note :: See [./terms-and-concepts.lex] for terminology used throughout.

1. The Built-in Handlers

    dodot ships with five handlers, covering the overwhelming majority of what dotfile repositories need.

    1.1. symlink

        Creates a symlink from a deployed location (typically `~/` or `~/.config/`) back to a file or directory in your pack. This is the default for any file that no other handler claims — anything that looks like plain configuration flows through here.

        Path resolution is smart: every pack-root entry — file or directory — defaults to `$XDG_CONFIG_HOME/<pack>/<name>` (so `nvim/init.lua` → `~/.config/nvim/init.lua`, `warp/themes/` → `~/.config/warp/themes/`). A small list of exceptions force `$HOME` placement regardless of XDG (`ssh`, `bashrc`, `zshrc`, etc.); the per-file `home.X` prefix and per-subtree `_home/` directory route opt-in single files or whole subtrees to `$HOME/.X`. For the full path rules, see [./symlink-paths.lex].

    1.2. shell

        Arranges for shell scripts to be sourced at login. Matches `{aliases,profile,login,env}.{sh,bash,zsh}` by default; add more patterns via `[mappings] shell` in `.dodot.toml`. The mechanism is a single `eval "$(dodot init-sh)"` line in your shell rc; the generated init script walks the datastore and emits `source` lines for every matched shell file.

        The extension convention is load-bearing: sourced files run in *your* shell, so `.zsh` files only parse cleanly in zsh sessions and `.bash` files in bash sessions. `.sh` is the portable bucket — use it for snippets that work in either. In practice most users run one shell, and the mismatch simply doesn't come up; users who switch shells occasionally can split their shell config by extension.

    1.3. path

        Exposes a directory on your `$PATH`. The conventional match is a `bin/` directory inside a pack; its contents become directly executable from any shell. Like shell, this rides on the dodot init script — the datastore records which directories should be on PATH, and the init script prepends them.

    1.4. install

        Runs an arbitrary shell script once, tracked by a sentinel file so it doesn't re-run on every deploy. Matches `install.sh`, `install.bash`, and `install.zsh` by convention. Use this for machine-specific setup that isn't covered by the other handlers: installing language toolchains, configuring window managers, creating directories, setting system defaults.

        The script's extension picks the interpreter — `.sh` and `.bash` run under `bash`, `.zsh` runs under `zsh` — not the user's login shell. An install script runs in a fresh subprocess, so the user's interactive shell state (aliases, functions, options) is not visible to it regardless; only the interpreter choice matters, and the extension is the contract the pack author declares.

        A pack with more than one matched install file (say, both `install.sh` and `install.zsh`) runs *all* of them, each tracked by its own sentinel. There is no "pick the best one" logic — if you only want one to run, only ship one.

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

3. Execution Order

    Within a single pack, handlers run in a fixed, documented order. The order is driven by an `ExecutionPhase` enum whose variants are declared in execution order — adding or moving a phase is a visible, deliberate code change, not an accident of alphabetical sort.

    Phases, in order:

        | Phase         | Handler   | Why here                                                           |
        | `Provision`   | homebrew  | Installs packages. Anything later may depend on tools it exposes.  |
        | `Setup`       | install   | User-authored scripts that can lean on Provision completing first. |
        | `PathExport`  | path      | Stages `bin/` onto PATH; runs before ShellInit.                    |
        | `ShellInit`   | shell     | Shell startup files that may reference binaries from PathExport.   |
        | `Link`        | symlink   | Catchall; must be last so precise handlers claim their files.      |

    :: table align=lll ::

    Two design invariants pin this order down.

    The catchall phase is always last.
        `symlink` is the only catchall handler (`MatchMode::Catchall`). Running it before any precise handler would let it claim files that belong elsewhere. `Link` sitting at the bottom of the enum is not a convention — it's the shape of "precise before catchall" written into the type.

    Code-execution phases run before configuration phases.
        `Provision` and `Setup` produce a filesystem a user's shell needs to see (installed binaries, `brew` formulae, generated files). `PathExport`, `ShellInit`, and `Link` deploy configuration that may reference those outputs. Reversing them would let a shell rc file try to source a program that hasn't been installed yet.

    The preprocessing layer (`.tmpl`, `.plist.xml`, `.age`) sits *upstream* of this ordering — templates are rendered before rules match, so by the time the phase order kicks in every match is a concrete file. See [./pre-processors.lex] for how preprocessors fit into the pipeline.

4. Cross-Pack Ordering

    Within a pack, handlers run in the phase order above. *Across* packs, dodot processes packs in lexicographic order of their on-disk directory names — and that order determines every cross-pack effect: shell init source order, `$PATH` entry order, install and homebrew execution order.

    Most users never have to think about this. The `nvim` pack and the `git` pack don't care whose shell snippets are sourced first, and lex order over readable names lands somewhere sensible.

    A few cases do care:

        - The Homebrew shell environment must be set up before any pack that calls `brew`.
        - `compinit` must run after completion-providing plugins but before `fzf-tab`.
        - On a fresh install, baseline setup (xcode-select, license acceptance) must precede anything that compiles.

    For those, dodot's stance is the one borrowed from `/etc/init.d` and `/etc/cron.d`: name your directories so lex order produces the order you want. Prefixing a few directories with three digits and a separator — `010-brew`, `100-zsh`, `900-starship` — works, and is the recommended pattern for the small minority of packs where ordering actually matters. Most setups get away with a handful of `0NN-`-prefixed baseline packs at the front; the rest stays unprefixed.

    :: note :: dodot does not have, and is not planning to add, a formal dependency graph or `before` / `after` declarations. The cost of getting those right is high, and the lex-order escape hatch handles the real cases.

    See [./../user/getting-started.lex] (Shell Integration) for what belongs *above* the dodot init line in your shell rc — the small set of bootstrap concerns that have to exist before dodot itself can run, and therefore can't live in a pack at all.

5. Configuration vs Code Execution

    Handlers fall into two categories that behave differently at deploy time. Category is derived from phase (`Provision` and `Setup` are Code Execution; the rest are Configuration).

    5.1. Configuration handlers

        symlink, shell, and path. Their operations are idempotent filesystem work: create a link, stage a file. Running them a second time produces the same result as running them once; no special tracking is required. `dodot up` always runs them in full.

    5.2. Code execution handlers

        install and homebrew. Their operations run user-authored shell commands. These are assumed _not_ to be idempotent in general — `install.sh` might install packages, write files, mutate the system — and re-running them on every `dodot up` would be slow, surprising, or both. Even Brewfile processing, though nominally idempotent, can take many seconds per pack.

        dodot solves this with sentinels. When a code-execution handler runs, it writes a small marker file to the datastore keyed by pack, handler, and a content hash of the command. On subsequent deploys, the presence of that sentinel causes the handler to skip. To override:

        - `--no-provision` skips code-execution handlers entirely for this run. Configuration handlers still run.
        - `--provision-rerun` forces code-execution handlers to run even when sentinels exist. Use after changing an install script, or to re-run `brew bundle` after adding a formula.

        When the content of a code-execution input changes (you edited `install.sh`, or the rendered output of `install.sh.tmpl` changed), the sentinel's content hash no longer matches, and the handler re-runs automatically. You only need `--provision-rerun` when you want to re-run without an input change.

6. Quick Reference

    Handler summary (rows in execution order):

        | Handler  | Phase       | Category       | Default claims                                     | Effect                              |
        | homebrew | Provision   | Code Execution | `Brewfile`                                         | `brew bundle` once per content hash |
        | install  | Setup       | Code Execution | `install.{sh,bash,zsh}`                            | Run once per content hash           |
        | path     | PathExport  | Configuration  | `bin/`                                             | Prepended to `$PATH`                |
        | shell    | ShellInit   | Configuration  | `{aliases,profile,login,env}.{sh,bash,zsh}`        | Sourced at shell login              |
        | symlink  | Link        | Configuration  | Anything else (catchall)                           | Link to `~` or `~/.config/`         |

    :: table align=lllll ::

7. Why Handlers Look the Way They Do

    A few design decisions worth naming.

    Handlers do not touch the filesystem.
        They read matches and produce intents. The actual work of creating links, running commands, and writing sentinels happens in layers below (executor, datastore). This keeps handlers small — each is a few dozen lines — and trivially testable without a real filesystem.

    Handlers are replaceable but not pluggable.
        The trait they implement is stable enough that writing a custom handler is not hard, but dodot does not load third-party handlers at runtime. The built-in set is deliberately small; we'd rather add handlers carefully than ship a plugin system we have to maintain.

    The catchall is always symlink.
        This is a convention rather than a hard rule of the code, but it's the only combination that preserves the "just name it sensibly" promise. If no precise handler matched, we know the user wanted the file deployed somewhere sensible, and a link is the right default.
