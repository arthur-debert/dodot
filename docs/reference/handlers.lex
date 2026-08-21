Handlers

    A handler is the thing that decides what to do with a file once dodot has decided to process it. Each handler has exactly one job: link configs, source shell scripts, add directories to `$PATH`, fetch external content, run install scripts once, install packages from a `Brewfile` or a `packages.nix`, or drop a file from processing entirely. This document describes the handlers dodot ships, the rules for how matches flow to them, and the distinction between handlers that always run and handlers that run once.

    See [./terms-and-concepts.lex] for terminology used throughout.

1. The Built-in Handlers

    dodot ships with ten handlers: seven that act on a file (symlink, shell, path, external, install, homebrew, nix) and three that drop it from processing (ignore, skip, gate). All ten share the same `Handler` trait and run from the same registry.

    1.1. Symlink

        Creates a symlink from a deployed location back to a file or directory in your pack. This is the default for any file that no other handler claims — anything that looks like plain configuration flows through here.

        Path resolution is smart: every pack-root entry — file or directory — defaults to `$XDG_CONFIG_HOME/<pack>/<name>` (so `nvim/init.lua` → `~/.config/nvim/init.lua`, `warp/themes/` → `~/.config/warp/themes/`). A small list of exceptions force `$HOME` placement regardless of XDG (`ssh`, `bashrc`, `zshrc`, etc.); the per-file `home.X` prefix and per-subtree `_home/` directory route opt-in single files or whole subtrees to `$HOME/.X`. For the full path rules, see [./symlink-paths.lex].

    1.2. Shell

        Arranges for shell scripts to be sourced at login. Matches `*.{sh,bash,zsh}` at the pack root by default — any shell-extension file at the top of a pack gets sourced. (`install.{sh,bash,zsh}` is claimed by the install handler first, at a higher priority, so install scripts are never accidentally sourced.) Add more patterns via `[mappings] shell` in `.dodot.toml`. The mechanism is a single `eval "$(dodot init-sh)"` line in your shell rc; the generated init script walks the datastore (see [./data-layer.lex]) and emits `source` lines for every matched shell file.

        The extension convention is load-bearing: sourced files run in *your* shell, so `.zsh` files only parse cleanly in zsh sessions and `.bash` files in bash sessions. `.sh` is the portable bucket — use it for snippets that work in either. In practice most users run one shell, and the mismatch simply doesn't come up; users who switch shells occasionally can split their shell config by extension.

    1.3. path

        Exposes a directory on your `$PATH`. The conventional match is a `bin/` directory inside a pack; its contents become directly executable from any shell. Like shell, this rides on the dodot init script — the datastore records which directories should be on PATH, and the init script prepends them.

    1.4. external

        Fetches content that upstream owns — a shell framework, a plugin-manager bootstrap, a community theme repo, a single shared snippet — and puts it where the rest of the pack can use it. Matches `externals.toml` at the pack root; each `[section]` in that file declares one resource, typed `file`, `git-repo`, `archive`, or `archive-file`.

        External is the one code-execution handler that refreshes on its own, because there is no user-authored code to re-run — only content to keep current. Each entry's sentinel records a content signature: `file`, `archive`, and `archive-file` use the sha256 you wrote in the TOML, so editing the TOML is the only thing that can cause a re-fetch; a `git-repo` asks upstream instead, via a cheap `git ls-remote` on every `up`, and re-fetches when that SHA moves. The decision is the handler's — `--provision-rerun` has no say in it. See [./../user/handlers/external.lex].

    1.5. install

        Runs an arbitrary shell script once, tracked by a sentinel file so it doesn't re-run on every deploy. Matches `install.sh`, `install.bash`, and `install.zsh` by convention. Use this for machine-specific setup that isn't covered by the other handlers: installing language toolchains, configuring window managers, creating directories, setting system defaults.

        The script's extension picks the interpreter — `.sh` and `.bash` run under `bash`, `.zsh` runs under `zsh` — not the user's login shell. An install script runs in a fresh subprocess, so the user's interactive shell state (aliases, functions, options) is not visible to it regardless; only the interpreter choice matters, and the extension is the contract the pack author declares.

        A pack with more than one matched install file (say, both `install.sh` and `install.zsh`) runs *all* of them, each tracked by its own sentinel. There is no "pick the best one" logic — if you only want one to run, only ship one.

        Output is quiet by default — start/end markers, the script's leading comment block, and any `# status: <message>` lines the script emits on stdout are surfaced live; everything else is captured and discarded unless the script fails (in which case stderr is dumped) or `dodot up --verbose` is passed (which streams the raw output). The `# status:` convention is tool-agnostic — the markers are plain shell comments when the script is run by hand. See [./../user/handlers/install.lex] for the user-facing details and examples.

    1.6. homebrew

        Runs `brew bundle --no-upgrade` against a `Brewfile`, once per content hash, with `HOMEBREW_NO_AUTO_UPDATE=1` in the environment. It is not a separate implementation: install, homebrew, and nix are one run-once handler parameterised by the command it builds. Editing the `Brewfile` does not re-run it — the new content hashes differently, so `dodot up` reports `brew packages older version` and holds, and `dodot up --provision-rerun` is what applies the edit.

        Homebrew is not macOS-only. It runs on Linux, and where its prerequisites are acceptable it is the same `Brewfile` you already have — but those prerequisites are real: a system compiler and build tools, and on older distributions brew brings its own gcc and glibc. Where that is too much to accept, `nix` covers the same need without leaning on the host toolchain, and `install.sh` remains the fallback for anything the distro's own package manager should own. See [./../user/handlers/homebrew.lex].

    1.7. nix

        Runs `nix profile install` against the pack's `packages.nix`, once per content hash — the same run-once machinery as install and homebrew, pointed at a different command. Matches `packages.nix` at the pack root. A pack can carry both a `Brewfile` and a `packages.nix`; the two run independently against their own package managers.

        Editing the manifest does not re-run it either: `dodot up` reports `nix packages older version` and holds until `--provision-rerun`. And the only command the handler can build is `nix profile install`, so *removing a line from `packages.nix` uninstalls nothing* — the re-run installs the remaining list, which adds nothing and removes nothing. Deleting `packages.nix` outright removes nothing either; provisioning state is deliberately left out of `up`'s wipe-and-reapply reconcile. See [./../user/handlers/nix.lex].

    1.8. ignore (filter)

        Claims matches and drops them silently — same contract as `.gitignore`. No executable intent, no entry in `dodot status`. Configured via `[mappings] ignore` (default empty). Useful for build artifacts, scratch files, anything you don't want dodot to know about.

    1.9. skip (filter)

        Claims matches, surfaces them in `dodot status` as `skipped`, but produces no executable intent. Configured via `[mappings] skip`. Defaults cover the common documentation/legal files (`README`, `LICENSE`, `CHANGELOG`, `CONTRIBUTING`, `AUTHORS`, `NOTICE`, `COPYING` and their `.*` variants), matched case-insensitively. Override per-pack with `skip = []` to deploy a README intentionally.

    1.10. gate (filter)

        Claims matches whose host predicate evaluates false on the current host — e.g. `install._darwin.sh` on a linux box, or any file inside `_darwin/` when running on linux. Surfaces in `dodot status` as `gated out (<label>)` with a footnote showing what the predicate expected vs what the host has. Produces no executable intent; the file stays on disk and will deploy on a matching host.

        Unlike `ignore` and `skip`, gate matches are *dynamic* — they depend on host facts (OS, arch, hostname, …) and on the filename grammar (`._<label>`, `_<label>/`) plus the `[mappings.gates]` config. The full surface is in [./../user/conditional-running.lex] and the design proposal at [./../proposals/shipped/conditional-running.lex]. The matching infrastructure is shared with `ignore` and `skip`: gate evaluation runs at scan time and produces a `RuleMatch` whose `handler` is `"gate"`, with predicate / host metadata stashed in `options` for the status renderer.

        The three filter handlers exist because four things were previously different mechanisms — a pack-level marker, a silent skip, a visible "excluded", and host-conditional dispatch — and unifying the intra-pack cases into real handlers means there's one matching model and one config grammar instead of four. Pack-level `.dodotignore` (the "pack-ignore" mechanism) and pack-level `[pack] os` (the conditional-running mechanism) stay separate at the discovery layer.

2. Matching Model

    Handlers are classified along two axes that together decide how matches flow.

    Match mode:
        _Precise_ handlers claim specific names or patterns: `install.sh`, `aliases.sh`, `Brewfile`, `bin/`. _Catchall_ handlers claim anything precise handlers didn't touch. Precise handlers run first and consume their matches; the catchall sees only what's left.

        At most one exclusive handler may be catchall, and the registry asserts it. Today that role is played by `symlink`. The constraint is a practical one: two catchalls would race for every unclaimed file.

    Scope:
        _Exclusive_ matches are consumed on first claim — no other handler sees that entry. _Shared_ matches remain available after a claim, so multiple handlers can act on the same entry. All current handlers are exclusive. Shared scope is reserved for future observer-style handlers (an audit handler, a stats handler) that watch without deploying.

    The scanner that produces matches works only at the pack's top level — it does not recurse. A handler that receives a directory entry decides how to treat its contents. The path handler stages the whole directory into `$PATH`; the symlink handler creates one symlink for the directory as a whole. If you want a nested path handled independently, you declare it explicitly in `.dodot.toml` (via `[symlink.targets]` or by naming a file inside it in `[symlink] protected_paths`).

3. Execution Order

    Within a single pack, handlers run in a fixed, documented order. The order is driven by an `ExecutionPhase` enum whose variants are declared in execution order — adding or moving a phase is a visible, deliberate code change, not an accident of alphabetical sort.

    The phase list itself — every phase in order, and which handlers sit in each — is generated from the registry at [./handler-registry.lex]. A phase may hold more than one handler: `Filter` holds three and `Provision` holds two. The order two handlers sharing a phase run in is not defined, and nothing in dodot depends on it.

    What the generated list can't tell you is why each phase sits where it does:

    `Filter`:
        Drops files before any deploying handler can claim them.

    `External`:
        Fetches the content upstream owns. It runs after `Filter` — no point fetching for a file that was dropped — and before `Provision`, so install scripts and shell init can rely on the fetched content already being in place.

    `Provision`:
        Installs packages. Anything later may depend on the tools it puts on disk.

    `Setup`:
        User-authored scripts, which can lean on `External` and `Provision` having completed first.

    `PathExport`:
        Stages `bin/` onto `$PATH`; runs before `ShellInit`.

    `ShellInit`:
        Shell startup files that may reference binaries from `PathExport`.

    `Link`:
        The catchall. Last, so every precise handler has claimed its files first.

    Three design invariants pin this order down.

    The filter phase is always first.
        `ignore`, `skip`, and `gate` exist to keep matched files away from deploying handlers. If they ran later, a precise mapping (priority 10) or the catchall (priority 0) could already have claimed the file and emitted intents. So `ignore` and `skip` sit at the highest priority tiers (100 and 50), and `gate` needs no rule at all — the scanner mints gate matches itself, before rule priorities come into play.

    The catchall phase is always last.
        `symlink` is the only catchall handler (`MatchMode::Catchall`). Running it before any precise handler would let it claim files that belong elsewhere. `Link` sitting at the bottom of the enum is not a convention — it's the shape of "precise before catchall" written into the type.

    Code-execution phases run before configuration phases.
        `External`, `Provision`, and `Setup` produce a filesystem a user's shell needs to see (fetched repos and archives, installed binaries, `brew` formulae, generated files). `PathExport`, `ShellInit`, and `Link` deploy configuration that may reference those outputs. Reversing them would let a shell rc file try to source a program that hasn't been installed yet.

    The preprocessing layer (`.tmpl`, `.plist.xml`, `.age`) sits *upstream* of this ordering — templates are rendered before rules match, so by the time the phase order kicks in every match is a concrete file. See [./pre-processors.lex] for how preprocessors fit into the pipeline.

4. Cross-Pack Ordering

    Within a pack, handlers run in the phase order above. *Across* packs, dodot processes packs in lexicographic order of their on-disk directory names — and that order determines every cross-pack effect: shell init source order, `$PATH` entry order, code-execution order.

    Most users never have to think about this. The `nvim` pack and the `git` pack don't care whose shell snippets are sourced first, and lex order over readable names lands somewhere sensible.

    A few cases do care:

        - The Homebrew shell environment must be set up before any pack that calls `brew`.
        - `compinit` must run after completion-providing plugins but before `fzf-tab`.
        - On a fresh install, baseline setup (xcode-select, license acceptance) must precede anything that compiles.

    For those, dodot's stance is the one borrowed from `/etc/init.d` and `/etc/cron.d`: name your directories so lex order produces the order you want. Prefixing a few directories with three digits and a separator — `010-brew`, `100-zsh`, `900-starship` — works, and is the recommended pattern for the small minority of packs where ordering actually matters. Most setups get away with a handful of `0NN-`-prefixed baseline packs at the front; the rest stays unprefixed.

    :: note :: dodot does not have, and is not planning to add, a formal dependency graph or `before` / `after` declarations. The cost of getting those right is high, and the lex-order escape hatch handles the real cases.

    4.1. The Prefix Grammar

        When a pack directory matches `^(\d+)[-_](.+)$` — that is, a run of digits followed by `-` or `_` followed by a non-empty stem — dodot treats the prefix as ordering metadata, not as part of the pack's logical name. Three things follow:

            - The full directory name is the *sort key*: `010-brew` < `020-zsh` < `nvim` < `starship`. Prefixed packs interleave with unprefixed ones via lex order, no special casing.
            - The *display name* is the stripped stem: `010-brew` → `brew`. That's what `dodot status`, `dodot list`, error messages, generated shell-init comments, and log lines all use. The prefix is invisible to every user-facing surface.
            - CLI arguments resolve against the display name first and fall back to the raw on-disk directory: `dodot up brew` and `dodot up 010-brew` find the same pack. The display form is the recommended one; the raw form keeps muscle-memory and scripts working.

        Symlink targets follow the display name too: a `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua` (where `nvim` actually reads its config), not `~/.config/010-nvim/init.lua`. The prefix lives on disk and on the sort axis; it does not leak into the user's filesystem.

        The 10/20/30 gap convention from `/etc/init.d` carries over: leave room between numbers so you can insert without renumbering. Three digits with leading zeros (`010`, `020`, `100`, `900`) keeps lex order matching numeric order past the 99 → 100 boundary; two digits work but break sort once you cross.

        Three classes of collision are rejected at scan time, with both offending paths in the error message:

            - A pack `nvim` and a pack `010-nvim` both exist — the display name `nvim` is ambiguous.
            - Both `010-nvim` and `020-nvim` exist — the display name `nvim` resolves to two packs.
            - A directory like `010-` or `010_` with no stem after the separator — a pack must have a name.

        Two packs with the same prefix and different stems (`010-brew` and `010-zsh`) are fine; lex order on the stem decides between them.

    See [./../user/getting-started.lex] (Shell Integration) for what belongs *above* the dodot init line in your shell rc — the small set of bootstrap concerns that have to exist before dodot itself can run, and therefore can't live in a pack at all.

5. Configuration vs Code Execution

    Handlers fall into two categories that behave differently at deploy time. Category is derived from phase (`External`, `Provision`, and `Setup` are Code Execution; the rest are Configuration).

    5.1. Configuration handlers

        symlink, shell, path, and the three filter handlers — ignore, skip, and gate. The first three do idempotent filesystem work: create a link, stage a file. Running them a second time produces the same result as running them once; no special tracking is required. The filter handlers do less than that: they claim a file and emit no intent at all, which is idempotent for the same reason. `dodot up` always runs all six in full.

    5.2. Code execution handlers

        install, homebrew, nix, and external. The first three run user-authored commands; the fourth pulls remote content into place. None are assumed idempotent — `install.sh` might install packages, write files, mutate the system — and repeating that work on every `dodot up` would be slow, surprising, or both. Even Brewfile processing, though nominally idempotent, can take many seconds per pack.

        dodot solves this with sentinels. When a code-execution handler acts, it writes a small marker file to the datastore keyed by pack, handler, and a content signature. On subsequent deploys, that sentinel is what decides whether to act again. Two flags override the decision:

        - `--no-provision` skips all four code-execution handlers entirely for this run. Configuration handlers still run. Each skipped file still gets a status row, labelled `skipped (--no-provision)`, so a run you asked to be partial doesn't read as an empty pack.
        - `--provision-rerun` forces the run-once handlers — install, homebrew, nix — to run even when sentinels exist. Use after changing an install script, or to re-run `brew bundle` after adding a formula. It has no effect on external, which decides for itself.

        The four split into two freshness rules, and the split is the thing to remember:

        *Run-once — install, homebrew, nix.* When the content of a run-once input changes (you edited `install.sh`, or the rendered output of `install.sh.tmpl` changed), the sentinel's content hash no longer matches — but the handler does *not* re-run on its own. `dodot up` reports the file as `older version` and holds it; `--provision-rerun` is what applies the edit. Running code you edited is always an explicit request.

        *External.* `externals.toml` entries refresh on their own, because there is no user-authored code to re-run — only content to keep current. Each entry's sentinel payload is its content signature, and what supplies that signature depends on the entry type. `file`, `archive`, and `archive-file` use the sha256 you wrote down, so editing the TOML is the only thing that can trigger a re-fetch. A `git-repo` tracks upstream instead, via a cheap `git ls-remote` on every `up` — against `HEAD`, or against the tag or branch in `ref` when the entry pins one — and refreshes when that SHA moves; an entry pinned to a `commit` polls nothing at all and refreshes only when you change the pin. Either way the decision is the handler's, and `--provision-rerun` has no say in it. See [./../user/handlers/external.lex] for the per-type detail.

6. Quick Reference

    The registry's own roster — every handler, its phase and category, its match mode and scope, what it claims by default, and what it does — is generated from the code at [./handler-registry.lex], along with the phase list and the default mappings. It is regenerated with `pixi run gen-docs` and a test fails when it drifts, so it cannot go stale the way a hand-copied table can.

7. Why Handlers Look the Way They Do

    A few design decisions worth naming.

    Handlers do not touch the filesystem.
        They read matches and produce intents. The actual work of creating links, running commands, and writing sentinels happens in layers below (executor, datastore). This keeps handlers small — each is a few dozen lines — and trivially testable without a real filesystem.

    Handlers are replaceable but not pluggable.
        The trait they implement is stable enough that writing a custom handler is not hard, but dodot does not load third-party handlers at runtime. The built-in set is deliberately small; we'd rather add handlers carefully than ship a plugin system we have to maintain.

    The catchall is always symlink.
        The registry allows at most one exclusive catchall and asserts it; symlink is the handler that fills the slot, and that is the only arrangement that preserves the "just name it sensibly" promise. If no precise handler matched, we know the user wanted the file deployed somewhere sensible, and a link is the right default.
