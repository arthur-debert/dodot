:: verified ::
Commands — index

The dodot command set, grouped the way `dodot --help` groups them. Most days you'll use `up`, `down`, and `status`. The remaining commands are for onboarding existing dotfiles, bootstrapping new packs, inspecting state, and wiring git up where dodot needs its cooperation.

All pack-based commands accept zero or more pack names. Without arguments they operate on every discovered pack. With arguments, they operate on just those packs.

For terminology, see [./glossary/pack.lex] and [./glossary/handler.lex].

1. Core

    - [./commands/up.lex] — deploy packs.
    - [./commands/down.lex] — remove deployed state for packs.
    - [./commands/status.lex] — show what dodot sees per pack. Read-only.
    - [./commands/list.lex] — enumerate visible packs.

2. Helpers

    - [./commands/adopt.lex] — move existing system files into a pack, leaving symlinks behind.
    - [./commands/init.lex] — create a new pack (directory + `.dodot.toml`).
    - [./commands/fill.lex] — add starter handler templates (`install.sh`, `aliases.sh`, `Brewfile`) to an existing pack.
    - [./commands/addignore.lex] — drop a `.dodotignore` marker so dodot stops discovering a directory.

3. Diagnostics

    - [./commands/probe.lex] — lower-level introspection: deployment-map, data-dir tree, shell-init timings, macOS app-support routing.

4. Git layer

    See [./commands/git-augmentation.lex] for the conceptual overview — the install ladder, the three rungs, and the Tier-2 alias.

    - [./commands/git-install-filters.lex], [./commands/git-show-filters.lex] — plist clean/smudge filters.
    - [./commands/git-install-alias.lex], [./commands/git-show-alias.lex] — the `git` shell alias that runs `dodot refresh` first.
    - [./commands/template.lex] — template clean filter + filter installer.
    - [./commands/transform.lex] — reverse-merge deployed edits to template sources, plus the pre-commit hook installer.
    - [./commands/plist.lex] — binary↔XML plist translators (the filter binary).
    - [./commands/prompts.lex] — inspect and reset dismissed prompts (including the install ladder).

5. Misc

    - [./commands/config.lex] — inspect, generate, or edit configuration.
    - [./commands/init-sh.lex] — print the shell integration script (you `eval` it from your rc).
    - [./commands/tutorial.lex] — interactive 10-minute walkthrough using your real dotfiles.
    - [./commands/refresh.lex] — touch source mtimes when deployed bytes diverged. Almost always wrapped in the Tier-2 alias.
    - [./commands/secret.lex] — inspect secret providers and template references. Read-only.

6. Global flags

    Every command accepts:

    - `--output <format>` — output format (`term`, `text`, `json`, `yaml`, `term-debug`).
    - `--verbose` — verbose logging to stderr.
    - `--debug` — debug logging to stderr (implies `--verbose`).
    - `--help` (or `-h`, or `dodot help <command>`) — per-command help with usage, options, examples, cross-references.

    The dotfiles root is not a flag. dodot resolves it by checking `$DOTFILES_ROOT` first, then `git rev-parse --show-toplevel`, then the current working directory. See [./glossary/dotfiles-root.lex].
