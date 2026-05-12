Alternatives — how dodot compares to the rest of the space

    Dotfile management is a crowded space. This document compares dodot against the seven alternatives most people end up choosing between: chezmoi, yadm, GNU Stow, dotbot, Home Manager, dotter, and dotdrop. Each comparison is anchored against dodot's design principles, listed in §2, so the question "does this tool work the way dodot works?" can be answered in a sentence per axis.

    :: note :: For dodot's own design rationale see [./philosophy.lex]; for terminology see [./terms-and-concepts.lex].

1. Where dodot sits

    The space breaks roughly into four clusters:

        | Cluster                  | Examples                         | Trait                                                 |
        | symlink farms            | GNU Stow, dotter                 | symlinks only, narrow scope, no apply step            |
        | YAML/TOML orchestrators  | dotbot, dotter, dotdrop          | a config file lists what to do                        |
        | source-state managers    | chezmoi, dotdrop                 | source ≠ deployed; an `apply` step renders/copies     |
        | git-tree-over-$HOME      | yadm                             | $HOME itself is the working tree of a bare git repo   |
        | declarative reproducible | Home Manager (Nix)               | rewrite config as Nix; user files become store links  |
    :: table align=ll ::

    dodot is its own row. It's a symlink manager that also handles the things-symlinks-can't-do (shell sourcing, PATH, install scripts, Brewfiles, plists) without crossing into source-state or declarative territory. The data layer ([./data-layer.lex]) is the trick that lets it do this: a "live receipt" double-link so the filesystem _is_ the state, without a database and without an apply step that has to be re-run after edits.

2. The principles to compare against

    These are the load-bearing positions in dodot's design. The rest of this document checks each alternative against them.

        | Principle                                                        | What it means                                                            |
        | Minimal requirements on file layout                              | group by directory, special filenames map to special actions             |
        | No mapping required, unless you want to customize                | conventional names = working setup with zero config                      |
        | No workflow change, no apply step                                | edit your files as you please; symlinks make edits live                  |
        | No tooling change — plain git, end-to-end                        | dodot never wraps, interprets, or interposes on git                      |
        | Three commands to learn — `status`, `up`, `down`                 | the cognitive load of routine use is small                               |
        | Correct path handling, including macOS Library / plist quirks    | XDG vs `$HOME`, Application Support, binary plists                       |
    :: table align=ll ::

    The advanced surface — templates, secrets, conditional running, plists — exists, but is designed to be reachable without breaking any of the above.

3. chezmoi

    chezmoi (https://chezmoi.io) is the most feature-rich tool in the space and the closest competitor in adoption. Written in Go.

    3.1. How chezmoi works

        Your source tree lives at `~/.local/share/chezmoi` and is _renamed_: `~/.zshrc` becomes `dot_zshrc` in the source, `~/.config/foo` becomes `dot_config/foo`, and a small vocabulary of prefixes (`private_`, `executable_`, `readonly_`, `encrypted_`, `symlink_`, `literal_`) encodes file attributes. Templates are flagged with a `.tmpl` suffix; one-shot scripts with `run_once_` / `run_onchange_`. `chezmoi apply` reads that tree and _writes regular files_ to your home directory (symlinks are an opt-in mode).

        State sits in a BoltDB file (`chezmoistate.boltdb`) tracking the hashes used to gate `run_once_` / `run_onchange_` scripts. Templates are Go `text/template` with chezmoi-specific functions for `.chezmoi.os`, `.chezmoi.hostname`, et al.

    3.2. Where chezmoi and dodot agree

        - Vanilla git for the source tree; both refuse to wrap commit/push.
        - First-class conditional deployment by OS / hostname / arch.
        - First-class secret integration with a wide range of providers (chezmoi covers more — 1Password, Bitwarden, AWS/Azure secret managers, Keychain, GNOME Keyring, etc.; dodot covers the popular subset — pass, 1Password, Bitwarden, sops, Keychain, secret-tool, age, gpg).
        - Whole-file encryption via age and gpg.
        - Templates with per-host content variation.
        - An import / adopt path for existing files.

    3.3. Where they differ

        File layout. chezmoi requires you to rename files into `dot_`-prefixed attribute syntax. A `.zshrc.tmpl` in source becomes `~/.zshrc`. dodot leaves your filenames alone: `git/gitconfig` deploys as `~/.gitconfig` because that's the convention dodot reads off the disk shape; the source path you see in the repo is the same path the editor sees later. Renaming is an explicit dodot opt-in (`home.X`, `_home/`, `_xdg/`, `_app/`) used when the default doesn't fit.

        Apply step. chezmoi has one; dodot does not. With chezmoi, deployed files are _copies_ of the rendered output; edits at `~/.zshrc` are not reflected in the source until you run `chezmoi re-add` (or `chezmoi merge`). The next `chezmoi apply` will _overwrite_ your edit if you didn't re-add it. dodot's symlink chain means the deployed file and the source file are the same byte stream — there is nothing to re-add and nothing to overwrite, because the round-trip never happens.

        State. chezmoi keeps a BoltDB; dodot keeps a directory of legible symlinks. The chezmoi DB can drift from reality (an entry says a script ran when its sentinel is stale, for example); dodot's "state IS the behavior" property forecloses that class of drift — see [./philosophy.lex] §5.

        Command surface. chezmoi is `add` / `apply` / `diff` / `status` / `edit` / `re-add` / `merge` / `forget` / `unmanage` / `update` / `verify` / `import` / `cd` / `git` / `state` / `execute-template` / `data` / `doctor` / `secret` / `chattr` — and that's the core. dodot is `status` / `up` / `down`, plus discoverable helpers (`adopt`, `init`, `fill`, `tutorial`).

        Shell, PATH, Brewfile, install.sh. chezmoi does not manage any of these natively; you write `run_onchange_install-packages.sh.tmpl` and the patterns are documented but unowned. dodot has first-class handlers for all four.

        macOS plists. chezmoi has no first-class plist story; the documented pattern is a `modify_` script that reads the existing plist with `plutil`/`PlistBuddy` and rewrites it. dodot ships git clean/smudge filters that bring binary plists under `git diff` review without leaving binary state in the repo — see [./../user/plists.lex].

    3.4. Things chezmoi can do that dodot doesn't

        Worth separating these by what kind of difference they are. Not every item on a "X can do Y, dodot can't" list is the same shape: some are structural gaps in dodot, some are intentional non-goals, some are mechanically easy to add as adoption grows, and some are convenience verbs over capability dodot already has.

            | Item                                                 | Kind                  |
            | External files pinned by URL or git ref              | Structural gap        |
            | Per-machine answers prompted at `chezmoi init`       | Convenience / UX      |
            | AWS/Azure/Keeper/LastPass/Dashlane/Doppler providers | Adoption-driven       |
            | Cross-platform Windows support                       | Intentional non-goal  |
        :: table align=ll ::

        Walking each:

        - *External files (structural).* chezmoi's `.chezmoiexternal.toml` declares resources that should exist on disk but are sourced from upstream — `type = "git-repo"` for oh-my-zsh, `type = "archive"` for a Nerd Font zip, `type = "file"` for a single URL'd file, `type = "archive-file"` for one file inside an archive. Each entry has a `refreshPeriod` so `chezmoi apply` re-checks upstream on a cadence. dodot has no analog. A dodot user covers the same need today with an `install.sh` that does `git clone && git pull` (content-hashed, but doesn't periodically refresh on its own) or a git submodule (manual `--remote` updates) or a Brewfile entry (when upstream ships through brew). The gap is real; adding it would mean a new handler with new failure modes (network, untrusted upstream, hash pinning), so the design cost is non-zero even if the implementation is small.
        - *Init-time prompts (convenience).* `chezmoi init` runs `.chezmoi.toml.tmpl` and prompts for any `promptStringOnce` calls, writing the answers to `~/.config/chezmoi/chezmoi.toml`. dodot already has the underlying mechanism — per-machine template vars under `[preprocessor.template.vars]` — but no interactive verb that asks for them on first run. Strictly a missing UX shortcut, not a capability gap.
        - *More secret providers (adoption-driven).* dodot ships six providers covering the most-used ground (pass, op, bw, sops, keychain, secret-tool) plus age/gpg whole-file. AWS Secrets Manager, Azure Key Vault, LastPass, Dashlane, Doppler, Keeper, etc. would be additional implementations of the same provider trait; they materialize as user demand surfaces.
        - *Windows (intentional non-goal).* dodot is unix-first by design; macOS plists and XDG/Library path resolution sit at the center of the value proposition. Windows is not on the roadmap.

        Once you discount the intentional non-goal and the adoption-driven item, **external files is the one structural capability dodot is missing**.

    3.5. A note on `chezmoi diff` and `chezmoi merge`

        `chezmoi diff` and `chezmoi merge` exist because chezmoi has an apply step: the source (`dot_zshrc.tmpl`) and the deployed (`~/.zshrc`) are different objects that drift the moment either side is edited, and git can only see the source. The chezmoi verbs are how the user asks "what would `apply` do?" and "reconcile my deployed edits into source." They are consequences of the apply-step model, not capability beyond what dodot offers.

        In dodot, divergence is structurally impossible for plain symlinked files — source and deployed are the same bytes. The only files where divergence *can* happen are preprocessed (`.tmpl`, `.age`, `.gpg`, plists), and dodot already covers that case via the git-augmentation stack ([./../user/commands/git-augmentation.lex]):

        - `dodot transform status` reports the synced/diverged matrix per cached preprocessed file (the `chezmoi diff` equivalent for the only files where it's relevant).
        - The template clean filter (`dodot template install-filter`) makes `git diff` show deployed-side template edits as if they were source-side, so vanilla `git diff` does the job at single-file granularity.
        - The pre-commit hook (`dodot transform install-hook`) runs `transform check --strict` automatically — either reverse-merging deployed-side edits into source or failing the commit with conflict markers. That's the `chezmoi merge` equivalent.
        - The Tier-2 shell alias (`dodot git-install-alias`) makes `git status` always reflect current truth without a manual `dodot refresh`.

        What a dodot user with the extensions installed *does* lose from chezmoi's verbs is ergonomic, not structural: chezmoi's `merge` can launch a configured three-way merge tool (vimdiff/meld/...) on `source ↔ destination ↔ target`, where dodot writes inline `dodot-conflict` markers into the source for you to resolve in your editor. Same outcome, different UI.

    3.6. Migration cost off each tool

        Off chezmoi: tractable. Deployed files are regular files (default mode), so they keep working. You undo the `dot_` rename of source files by hand or with a one-time script. The Go template syntax stays put unless you replace it.

        Off dodot: trivial. `dodot down` removes everything dodot owns; the dotfiles repo is plain files with conventional names. No renames to undo.

4. yadm

    yadm (https://yadm.io) is "Yet Another Dotfiles Manager." A single Bash script that wraps git.

    4.1. How yadm works

        yadm makes `$HOME` itself the working tree of a bare git repo stored at `~/.local/share/yadm/repo.git`. Your `~/.zshrc` is the tracked file — no source-state directory, no symlinks, no renames. `yadm` commands fall in three categories: yadm-specific verbs (`alt`, `encrypt`, `bootstrap`), git-passthrough verbs (`add`, `commit`, `push`, `status`) executed with `--git-dir` and `--work-tree` pre-set, and a few small mediators (`gitconfig`, `enter`).

        Per-host variation rides on "alt files": `~/.zshrc##os.Darwin` is symlinked to `~/.zshrc` on macOS via `yadm alt`. Suffix grammar covers OS, hostname, distro, class, arch, user, extension, template engine.

    4.2. Where yadm and dodot agree

        - Plain git underneath; no committed-format lock-in.
        - The dotfiles repo is the source of truth, not a database.
        - Vanilla git commands still work alongside the tool.
        - A whole-pack-equivalent gate by hostname / OS / arch exists.
        - Adopting existing files is trivial (files are already at their target paths).
        - Low command surface — but bigger than dodot's three.

    4.3. Where they differ

        Git wrapping. yadm is a git wrapper by design — `yadm add`, `yadm commit`, `yadm push` are the canonical surface. You _can_ use plain `git` with `--git-dir`/`--work-tree`, but that's friction, not the primary path. dodot refuses to wrap git at all; you run `git commit` the way you run `git commit` in any other repo.

        Source tree is `$HOME`. With yadm there is no separate dotfiles directory you can browse without git noise. With dodot, your packs are visible top-level directories of a dotfiles repo you can navigate, search, and grep without git in the picture.

        Shell, PATH, Brewfile, install.sh. yadm has `bootstrap` (a single script that runs once after `clone`). It is yours to write — there are no handlers for `*.sh` sourcing, PATH dirs, Brewfile, or content-hashed re-runs.

        Templates. yadm's "default" template engine is an awk-based Jinja-lite; it also supports esh, j2cli, envtpl. It works, but is less ergonomic than chezmoi's Go templates or dodot's MiniJinja. No secret-function inside templates.

        Secrets. yadm offers `yadm encrypt`/`decrypt` (a single tar archive of globbed files, GPG-encrypted, committed as one blob), plus thin wrappers around `transcrypt` and `git-crypt`. No integration with `pass`, 1Password, Bitwarden, sops, Keychain, age, etc.

        macOS plists. None.

        State semantics. yadm's "state" is git itself plus the alt symlinks; that's coherent, but the alt mechanism is the only handler-like piece. Anything dodot does past pure file deployment (shell init, PATH, install hashing, Brewfile sentinels, plist filters) yadm does not do.

    4.4. Things yadm can do that dodot doesn't

        - Files live at their canonical path in `$HOME`. No symlinks, ever — programs that struggle with symlinks (sandboxed apps, some editors) see real files.
        - The setup story is genuinely one command: `yadm clone <url>` and you're in.
        - Direct edits to `~/.zshrc` are direct commits-in-waiting; no symlink chain to think about.

    4.5. Migration cost

        Off yadm: nothing to do. Files are already at their canonical paths. Delete the bare repo if you want. Alt-file symlinks remain as plain symlinks (or delete those by hand).

        Off dodot: `dodot down` reverses everything; the dotfiles repo is portable.

5. GNU Stow

    GNU Stow (https://www.gnu.org/software/stow/) is the original symlink farm manager. Written in Perl for managing per-package installs under `/usr/local`; the dotfiles community repurposed it.

    5.1. How Stow works

        Each "package" is a subdirectory of the stow directory whose internal tree _mirrors_ the target tree exactly. `dotfiles/vim/.vimrc` is symlinked to `~/.vimrc` when you `stow vim`. Stow tries to fold whole subtrees into a single symlink when nothing collides ("tree folding"), and splits them apart when a second package needs to share the subtree ("unfolding").

        That's it. No state, no templates, no scripts, no secrets, no conditional logic, no shell/PATH integration, no plist handling, no package install hooks.

    5.2. Where Stow and dodot agree

        - Symlinks are the deployment mechanism; both keep state in the filesystem.
        - No database, no apply step beyond `stow <pkg>`.
        - Lock-in is essentially zero (Stow has none; dodot has the datastore directory but it dies cleanly).
        - Vanilla git for source control.

    5.3. Where they differ

        Scope. Stow only does symlinks. Anything past that — sourcing shell snippets, adding to PATH, running Brewfile, dealing with plists, decrypting secrets, host conditionals — is not in Stow's lane and never will be. dodot does all of those as first-class handlers.

        Path layout. Stow requires you to mirror the target tree exactly inside each package: `vim/.vimrc`, `vim/.config/nvim/init.vim`. dodot reads pack-root entries with XDG defaults: `nvim/init.lua` deploys to `~/.config/nvim/init.lua` automatically. The `.config` mirroring step that Stow needs is dodot's default; the `~/.bashrc` mirroring that Stow needs is dodot's `home.X` opt-in or `force_home` list.

        Adopt. `stow --adopt` exists. It does the _opposite_ of what most users expect: it moves the live file _into_ the package, overwriting whatever was already there in the repo. Without a `git diff` save, you lose the repo's prior contents. dodot's `adopt` is the conservative direction — move the live file into the pack, replace the source with a symlink back, refuse if names collide.

        Conventional names. Stow doesn't have any: every file is just a path mirror. `Brewfile`, `install.sh`, `aliases.sh`, `bin/` are inert filenames to Stow, deployed as their literal selves.

        Target default. Stow's default target is the _parent_ of the stow directory, not `$HOME`. You almost always need `-t ~` or run from a specific layout. dodot defaults to `$HOME` everywhere.

        Per-host. Stow has no built-in conditional. Multi-machine workflows are "different packages per machine, stow selectively." dodot ships filename grammar, directory segments, glob tables, and `[pack] os` for this.

    5.4. Things Stow can do that dodot doesn't

        - Run on a 25-year-old box with only Perl installed.
        - Manage software installs under prefixes that aren't `$HOME` (it's a `/usr/local`-style symlink farm too).
        - Tree folding — one directory symlink can replace many file symlinks. dodot can also wholesale-link directories, but Stow's folding/unfolding is its defining trick.

    5.5. Migration cost

        Off Stow: trivial. Symlinks are plain, deletable with `stow -D` or `rm`. Files in the stow dir are plain files.

        Off dodot: trivial. Same story.

6. dotbot

    dotbot (https://github.com/anishathalye/dotbot) is a YAML-driven symlink-and-script bootstrapper, typically vendored as a git submodule.

    6.1. How dotbot works

        An `install.conf.yaml` lists directives in order: `link:` (the dominant one), `create:`, `shell:`, `clean:`, `defaults:`, `plugins:`. `link:` maps a path in `$HOME` to a path in the repo; `shell:` invokes arbitrary shell commands; `clean:` deletes dead symlinks under a directory. You run `./install` (the bundled shim) and dotbot reconciles. Every run is full re-evaluation; there's no state file.

    6.2. Where dotbot and dodot agree

        - Symlinks are the primary deployment mechanism.
        - Vanilla git.
        - Idempotent re-runs.
        - Low lock-in.

    6.3. Where they differ

        Mapping is mandatory. Every dotfile you want deployed must be listed under `link:` (or matched by glob). There is no convention-based dispatch. dodot's whole point is that you don't list anything — the file's name is the configuration.

        Templates, secrets, encryption. None native. There are plugins (`dotbot-template`, `dotbot-age`, `dotbot-sops`, `dotbot-gitcrypt`) for each, but they're community-maintained and you wire them up.

        Conditional running. Per-directive `if:` taking a shell snippet. Cross-platform repos typically end up with `if: '[[ "$(uname)" == "Darwin" ]]'` strewn across the YAML, or split into multiple top-level configs and a wrapper.

        Shell integration / PATH. No first-class handler. You write a `shell:` entry that appends to `.bashrc` or `.zshrc`, or you symlink your own init file that does it.

        Package install. `shell:` entry running `brew bundle`, `apt-get install`, etc. The plugin ecosystem covers most package managers — `dotbot-brew`, `dotbot-apt`, `dotbot-dnf`, `dotbot-pip` — but you pick and wire.

        One-shot semantics. `shell:` runs every time. You make it idempotent by hand (`command -v fzf || install`). dodot's install handler is content-hashed: edit the script, and it re-runs; leave it alone, and it doesn't.

        macOS plists. None. `defaults write` from a `shell:` entry is the documented approach.

        Adopt. No `import` workflow. You move files into the repo by hand and add YAML entries.

        Command surface. Effectively `./install` plus flags. Smaller than dodot's three commands at first glance, but every dotfile and every action you want is a YAML entry to author and maintain — the surface migrates from CLI verbs to config schema.

    6.4. Things dotbot can do that dodot doesn't

        - The vendored-submodule pattern means the dotfiles repo is fully self-contained; clone and `./install` works without installing anything system-wide. dodot is a binary you install.
        - A YAML manifest is sometimes the right thing — explicit, ordered, easy to grep for "what does this repo deploy."
        - A wide plugin ecosystem of small focused tools you can mix.

    6.5. Migration cost

        Off dotbot: light. Delete the `dotbot/` submodule and the `install` shim. Files in the repo are plain files. The YAML is dotbot-specific but is documentation, not deployed artifact.

        Off dodot: light. Same.

7. Home Manager

    Home Manager (https://github.com/nix-community/home-manager) is the Nix-based user-environment manager. The most philosophically different tool in this comparison.

    7.1. How Home Manager works

        You write your user environment in the Nix language: `programs.git.enable = true; programs.git.userName = "Arthur";`, `home.packages = [ pkgs.ripgrep pkgs.fd ];`, `home.file."some/path".text = '' ... '';`. `home-manager switch` evaluates the Nix expression, builds an immutable artifact in `/nix/store`, and atomically swaps activation symlinks so your `$HOME` files point into that store path. Each switch is a numbered _generation_; `home-manager generations` lists them; `home-manager switch --rollback` reverts to the previous one. The Nix store is the state.

    7.2. Where Home Manager and dodot agree

        - Source of truth is git (your Nix files).
        - First-class conditional logic (Nix `if`/`mkIf`/`mkMerge` vs dodot gates).
        - First-class package installation.
        - First-class shell integration (`programs.zsh`, `sessionVariables`, `home.sessionPath`).
        - Per-host modules for multi-machine setups.

    7.3. Where they differ — and they differ a lot

        Workflow change. This is the load-bearing one. With Home Manager you _stop hand-editing config files_. `~/.zshrc` is a read-only symlink into `/nix/store`. Editing your zsh config means editing `programs.zsh.initExtra = '' ... '';` in Nix and rebuilding. dodot's central promise — edit files the way you already do; the symlink chain makes the edit live — is exactly the property Home Manager trades away for reproducibility.

        Apply step. Home Manager has one — `home-manager switch` — and it is non-trivial. It evaluates Nix (can be slow), builds derivations, and rewrites symlinks. dodot has `dodot up` once at setup; after that, edits go live without re-running.

        Language. You learn Nix. The schema for `programs.git.*` is a Nix-typed surface that hides the underlying `.gitconfig` syntax; coverage is wide but not total, so you fall back to `extraConfig` strings for the long tail.

        Lock-in. Every Home Manager-deployed file is a symlink into `/nix/store`. Remove Nix and every managed file goes dangling. Migrating off requires `cp -L` to materialize each file as a real file before uninstalling. dodot's footprint is a directory of legible symlinks in `~/.local/share/dodot/`; uninstalling dodot leaves a working system because the symlinks resolve directly into your dotfiles repo.

        Adopt. Home Manager has no adoption workflow. You translate existing files into Nix expressions by hand, or stuff them verbatim into `home.file."...".text = '' ... '';`. Existing files in `$HOME` either get clobbered (`backupFileExtension` is set) or block activation.

        macOS plists. Home Manager itself doesn't own macOS system surfaces — its sibling project nix-darwin does. The pair (nix-darwin + Home Manager) covers plists; Home Manager alone doesn't. dodot's plist filter pipeline is self-contained.

        Command surface. ~10 verbs, but the cognitive load is mostly in the Nix expression language, not the verbs.

    7.4. Things Home Manager can do that dodot doesn't

        Three of these four are intentional non-goals for dodot — features that would require accepting the Nix tax (a 6-GB package manager, a workflow shift away from hand-edited config files, deployed files that become read-only store symlinks). The remaining one is a structural choice with a small workaround.

            | Item                                       | Kind                              |
            | Byte-identical reproducibility (flake.lock) | Intentional non-goal             |
            | Atomic rollback to a prior generation       | Intentional non-goal             |
            | Typed per-program schemas                   | Intentional non-goal             |
            | Unified package install (`home.packages`)   | Architectural difference         |
        :: table align=ll ::

        - *Reproducibility (intentional non-goal).* A pinned `flake.lock` plus `home.nix` reproduces the same environment on another machine, byte for byte. dodot offers reproducibility-by-convention (your dotfiles repo + a stable system) but not byte-identical guarantees — that property requires content-addressed package storage, which requires Nix. [./philosophy.lex] §7 ("no central orchestration") and the broader anti-DSL stance imply dodot won't go there.
        - *Atomic rollback (intentional non-goal).* `home-manager switch --rollback` reverses to the previous generation in one command. dodot's philosophy §7 explicitly takes the other side: "git keeps history of your configs; that is the history you want." Adding generation-based rollback would mean dodot owning a history of past deployments, which is exactly the bespoke-state-representation the design rules out.
        - *Typed per-program schemas (intentional non-goal).* `programs.git.signing.key = "...";` is type-checked, discoverable in the module registry, and composable across hosts. The trade-off: you author the schema's Nix surface, not the program's native config language; coverage is wide but lags upstream features. dodot reads existing files in their native formats and stays out of the schema-wrapping business — same posture as "no DSL" in philosophy §2.
        - *Unified package install (architectural difference).* `home.packages = [ ... ];` lives in the same declarative tree as everything else; there's no separate Brewfile concept. dodot keeps the boundaries between handlers crisp: `Brewfile` → homebrew handler, `install.sh` → install handler. The result is two manifests instead of one, but each speaks its native language (Brewfile syntax for brew, shell for one-shot setup) rather than going through a unified Nix expression. This is a different design choice rather than a capability gap; the surface area on dodot's side is the same, just split.

    7.5. Migration cost

        Off Home Manager: non-trivial. `cp -L` every symlink into a real file before removing Nix, then audit for missed pieces. Or migrate to another tool that can ingest your now-materialized files. Users have written posts about this; the consensus is "do the work or stay."

        Off dodot: trivial. `dodot down` and walk away.

8. dotter

    dotter (https://github.com/SuperCuber/dotter) is a small Rust-based templater + symlink manager. Less feature-rich than chezmoi by design.

    8.1. How dotter works

        Two TOML files: `.dotter/global.toml` (committed) declares packages with `files` (source → target maps) and `variables`. `.dotter/local.toml` (per-machine, gitignored) selects which packages to deploy and overrides variables. `dotter deploy` either symlinks each file or, if the file contains `{{`, renders it as a Handlebars template and writes the rendered output. Detection is content-based: presence of `{{` flips the file to template mode.

        Per-file Complex Targets let you force `symbolic` / `template`, set `if = "(eq shell 'bash')"` predicates, and override paths. Four hook scripts (`pre_deploy.sh`, `post_deploy.sh`, and the undeploy pair) run on each lifecycle event. A small cache at `.dotter/cache.toml` tracks deployment state.

    8.2. Where dotter and dodot agree

        - Rust binary, single static install.
        - Symlinks by default.
        - Vanilla git for source control.
        - Cache/state is small and visible.

    8.3. Where they differ

        Mapping is mandatory, in TOML. Every file you want deployed has to appear under `[default.files]` (or another package's `files`) with explicit `src` → `target`. dodot's filename-convention dispatch covers the same ground without any per-file declaration.

        Template detection. dotter sniffs content for `{{`; dodot uses filename suffix (`.tmpl` / `.template`). The content sniff is more magic and has the failure mode where any file that legitimately contains `{{` (e.g. Vim configs, Mustache examples) accidentally becomes a template.

        Conditional deployment. dotter's `local.toml` selects whole _packages_; finer-grained gating is via Complex Target `if = "..."` predicates. dodot has filename suffixes, directory segments, pack-level `[pack] os`, and a glob table — multiple surfaces matched to different granularities.

        No secrets, no encryption. Not in scope.

        No shell handler, no PATH handler, no install handler, no Brewfile handler. dotter's deployment is "deploy each file"; anything past that goes into `post_deploy.sh`, which you write.

        No adopt workflow. Move files into the repo by hand; `dotter deploy -f` then replaces the existing target files with symlinks.

        macOS plists. No special handling.

    8.4. Things dotter can do that dodot doesn't

            | Item                                                  | Kind                     |
            | Custom Handlebars helpers in Rhai                      | Adoption-driven          |
            | Per-machine target path overrides (`local.toml`)       | Adoption-driven          |
        :: table align=ll ::

        - *Custom helpers (adoption-driven).* dotter lets you write `name = "path/to/script.rhai"` under `[helpers]` and call the function from a Handlebars template. dodot's MiniJinja templates ship a built-in filter set and don't accept user-supplied helpers; adding a hook for it is a small extension, not a structural change.
        - *Per-machine target overrides (adoption-driven).* dotter's `local.toml` is gitignored and overrides target paths per host (useful when `init.vim` lives at different paths on Linux vs Windows). dodot's `[symlink.targets]` is the equivalent surface but lives in committed config; per-machine overrides today go through template-rendered config or hostname-gated alternate files. A gitignored local-overlay file (a kind of `.dodot.local.toml`) would close this cleanly.

    8.5. Migration cost

        Off dotter: light. `dotter undeploy` removes the deployed symlinks; the repo is plain files (with Handlebars `{{ }}` markers in templated ones).

        Off dodot: trivial.

9. dotdrop

    dotdrop (https://github.com/deadc0de6/dotdrop) is a Python-based dotfile manager with first-class profile support and Jinja2 templating.

    9.1. How dotdrop works

        Config is a YAML file (`config.yaml`) with sections: `dotfiles:` (each entry has `src`, `dst`, `link`, `actions`, transformations, ignore globs); `profiles:` (per-host or per-role selections of dotfile keys); `actions:` (named shell pre/post commands); `trans_install` / `trans_update` (decrypt / encrypt command pairs, typically gpg); `variables`, `dynvariables` (shell-executed). Files live under a `dotpath` directory (default `dotfiles/`). `dotdrop install` materializes each dotfile to its `dst` — either copied (with template render), or symlinked. `dotdrop import` is the inverse: ingest an existing file from `$HOME` into the repo, generating a dotfile key.

    9.2. Where dotdrop and dodot agree

        - Vanilla git.
        - Templates (Jinja2) for per-host variation.
        - First-class adopt-existing (`dotdrop import`).
        - Per-machine differentiation (profiles).
        - Secrets via external tools (dodot: pass/op/bw/sops/keychain/secret-tool + age/gpg; dotdrop: gpg-via-transformations + env vars).
        - Low lock-in.

    9.3. Where they differ

        Mapping is mandatory, in YAML. Every dotfile is an entry under `dotfiles:`; every profile lists which dotfile keys it includes. Importing through `dotdrop import` generates the entry for you, but the entry is still the configuration. dodot's convention-based dispatch has no equivalent registry.

        Profiles. dotdrop has first-class profiles — multiple distinct configurations selected by name (default = hostname). dodot is explicitly single-config-per-machine ([./philosophy.lex] §7); hostname-based gates are the closest analog. If you want "work" and "home" on the same laptop and `dotdrop -p work` / `dotdrop -p home` switching them, dotdrop wins.

        Templating triggers. dotdrop templates _every_ file by default (controlled by `template: false` or global `template_dotfile_default`). dodot opts in via `.tmpl`/`.template` extension. The dotdrop default makes accidental templating possible in files that contain `{{@@` or unusual Jinja-adjacent syntax; the custom `{{@@ … @@}}` delimiters reduce but don't eliminate that.

        Shell integration / PATH / Brewfile. No first-class handlers. `actions:` (pre/post shell commands) cover everything beyond file deployment — package installs, sourcing setups, etc., all become custom action lines.

        Install gating. `actions:` run on every install unless wired through state-checking shell logic by hand. dodot's install handler is content-hashed and skipped automatically when nothing changed.

        macOS plists. None.

        Deployment mode. dotdrop's `link` directive picks per-dotfile: `nolink` (copy), `absolute`, `relative`, `link_children`. Default is `nolink` — copy semantics, with the same "edits at the deployed location don't flow back to source" issue chezmoi has. Flip `link:` to opt into the dodot-like model.

    9.4. Things dotdrop can do that dodot doesn't

            | Item                                                 | Kind                  |
            | Multi-profile-per-machine (`-p` switching)            | Intentional non-goal  |
            | Profile inheritance + cross-profile composition       | Intentional non-goal  |
            | Dynamic vars (`dynvariables` from shell commands)     | Adoption-driven       |
            | Symlink-of-children (`link_children`)                 | Possible enhancement  |
        :: table align=ll ::

        - *Multi-profile (intentional non-goal).* dotdrop's `-p work` / `-p home` switching is its strongest feature for users who want different configurations on the same machine. dodot is explicitly single-config-per-machine — [./philosophy.lex] §7: "no profiles. One configuration per machine. For work-vs-home, use separate packs and enable different subsets." The hostname-based gate is the deliberate stopping point.
        - *Profile inheritance / composition (intentional non-goal).* Falls out of the same decision. Without profiles as a first-class concept, `include` chains and cross-profile composition have nothing to compose.
        - *Dynamic variables (adoption-driven).* dotdrop's `dynvariables` execute a shell command at render time and capture stdout as the variable value. dodot has `env.X` lookups in templates (resolved at `dodot up` time) but not arbitrary shell-command-as-value. Could be added as a new template function (`shell("date +%Y")` or similar) without disturbing other principles; just hasn't been needed yet.
        - *link_children (possible enhancement).* dotdrop's `link_children` symlinks each child of a directory independently, so the parent stays a real directory shared with non-managed siblings. dodot today links whole directories wholesale when the pack supplies a directory; per-child linking would be a small extension to the symlink handler.

    9.5. Migration cost

        Off dotdrop: light. `dotdrop uninstall` removes deployed files. `dotpath/` is plain files (with Jinja delimiters in templated ones).

        Off dodot: trivial.

10. Summary table

    Rows are features; columns are tools. "yes" means the tool supports the feature out of the box. Annotations explain conditions.

    10.1. Source layout & deployment

        | Feature                                                | dodot                                  | chezmoi                                | yadm                          | Stow                          | dotbot                        | Home Manager                       | dotter                         | dotdrop                       |
        | Edit your files at their normal paths                  | yes (symlinks)                         | no (apply rewrites them)               | yes (live in $HOME)           | yes (symlinks)                | yes (symlinks)                | no (read-only Nix store)           | yes (symlinks)                 | yes if `link:` set; else no   |
        | No apply step (edits go live)                          | yes                                    | no (`apply` required)                  | yes                           | yes                           | yes                           | no (`switch` required)             | yes (symlink mode)             | yes (link mode) / no (copy)   |
        | Zero-config for conventional dotfiles                  | yes                                    | no (rename to `dot_X`)                 | yes                           | no (mirror tree)              | no (YAML entries)             | no (Nix module)                    | no (TOML entries)              | no (YAML entries) / yes via import |
        | Filename remap required                                | no                                     | yes (`dot_`, `private_`, …)            | no                            | no                            | no                            | no (Nix attr)                      | no                             | no                            |
        | Source files keep their normal contents (no markup)    | yes (except `.tmpl`)                   | yes (except `.tmpl`)                   | yes (except alt suffix files) | yes                           | yes                           | no (Nix language)                  | yes (except `{{`-bearing)      | yes (except templated)        |
    :: table align=ll ::

    10.2. Source-control posture

        | Feature                                | dodot              | chezmoi              | yadm                            | Stow              | dotbot           | Home Manager       | dotter           | dotdrop            |
        | Plain `git` works alongside the tool   | yes                | yes                  | yes (with --git-dir flags)      | yes               | yes              | yes                | yes              | yes                |
        | Tool wraps git commands                | no                 | optional (auto-commit) | yes (primary surface)         | no                | no               | no                 | no               | no                 |
        | Tool offers its own diff/status verbs  | no                 | yes                  | passthrough only                | no                | no               | yes (generations)  | no               | yes (`compare`)    |
    :: table align=ll ::

    10.3. Features (built-in)

        | Feature                                | dodot                          | chezmoi                          | yadm                                | Stow      | dotbot                       | Home Manager                       | dotter                       | dotdrop                              |
        | Templates                              | yes (MiniJinja, `.tmpl`)       | yes (Go text/template, `.tmpl`)  | yes (default awk, esh, j2, envtpl)  | no        | no (plugins exist)           | no template engine (use Nix)       | yes (Handlebars, content-sniff) | yes (Jinja2, default-on)          |
        | Per-OS/host conditionals               | yes (filename + config)        | yes (template if)                | yes (alt suffix)                    | no        | yes (per-entry shell `if:`)  | yes (Nix `mkIf`)                   | yes (`if =` predicates)      | yes (profiles)                       |
        | Profiles on same machine               | no (single config)             | template branches                | class                               | no (split packages) | multi-config files     | per-host modules                   | local.toml                   | yes (first-class)                    |
        | Shell sourcing handler                 | yes (`*.sh|.bash|.zsh` at root)| no                               | no                                  | no        | no (shell directive)         | yes (`programs.zsh.initExtra`)     | no                           | no                                   |
        | $PATH handler                          | yes (`bin/` dirs)              | no                               | no                                  | no        | no                           | yes (`home.sessionPath`)           | no                           | no                                   |
        | Run-once install script                | yes (`install.sh`, hashed)     | yes (`run_once_`)                | yes (single `bootstrap`)            | no        | no (every run, hand-idempotent) | yes (`home.activation`)         | no (every run via hooks)     | partial (actions; not auto-hashed)   |
        | Brewfile / package install             | yes (`Brewfile` handler)       | run_onchange pattern (manual)    | bootstrap (manual)                  | no        | shell directive / plugin     | yes (`home.packages`)              | no                           | no (actions)                         |
        | Secrets — value injection              | yes (pass, op, bw, sops, keychain, secret-tool) | yes (~15+ providers)        | no                                  | no        | no (plugins)                 | no native (sops-nix/agenix module) | no                           | partial (env var, gpg via trans)     |
        | Whole-file encryption                  | yes (age, gpg)                 | yes (age, gpg)                   | yes (gpg tar, transcrypt, git-crypt) | no       | no (plugins)                 | sops-nix / agenix                  | no                           | yes (gpg via `trans_install`)        |
        | macOS plist diff/review                | yes (clean/smudge filters)     | no (modify-script pattern)       | no                                  | no        | no                           | nix-darwin (sibling project)       | no                           | no                                   |
        | Adopt existing files                   | yes (`dodot adopt`)            | yes (`chezmoi add`)              | trivial (already in $HOME)          | yes (`--adopt`, but inverted) | no (manual)         | no                                 | no                           | yes (`dotdrop import`)               |
        | External files (URL / git ref)         | no                             | yes (`chezmoiexternal`)          | no                                  | no        | plugin (`dotbot-git`)        | yes (`fetchurl` / `fetchFromGitHub`) | no                         | partial (config imports)             |
    :: table align=ll ::

    10.4. Cognitive load & portability

        | Feature                                | dodot                  | chezmoi              | yadm                          | Stow                | dotbot                   | Home Manager                       | dotter             | dotdrop              |
        | Core command count                     | 3 (`status`/`up`/`down`) | ~20+               | ~17 + git passthrough         | 1 modal             | 1 (`./install`)          | ~10                                | 6                  | ~10                  |
        | State storage                          | datastore symlinks     | BoltDB               | git repo                      | none                | none                     | Nix store generations              | `.dotter/cache.toml` | workdir + optional backups |
        | Runtime dependency                     | rust binary (~5MB)     | go binary            | bash + git                    | perl                | python                   | nix (multi-GB)                     | rust binary        | python + libmagic + diff |
        | Migration off the tool                 | `dodot down` + delete repo dir | un-rename `dot_` + edits stick | nothing (already in place) | `stow -D` or `rm`   | delete submodule         | `cp -L` every symlink first        | `dotter undeploy`  | `dotdrop uninstall`  |
        | Files dangling if tool uninstalled     | no (link directly to repo) | no (real files)  | n/a (real files)              | no (real symlinks)  | no                       | YES (point into `/nix/store`)      | no                 | no                   |
    :: table align=ll ::

11. The shape of the choice

    A few patterns fall out of the table.

    - If you want "edit normally, symlinks make it live, conventional names just work" with nothing else: dodot or Stow. Stow can't go past plain symlinks; dodot covers the next layer (shell, PATH, install, Brewfile, plist, templates, secrets, conditional running) without breaking the contract.
    - If you want one source of truth that produces byte-identical environments across machines: Home Manager. Pay the Nix tax.
    - If you want files to literally _be_ at their canonical paths with git as the only abstraction: yadm.
    - If you want the largest feature surface and don't mind an apply step plus filename renames: chezmoi.
    - If you want explicit YAML/TOML manifests that read like a deployment script: dotbot, dotter, or dotdrop. dotdrop has the strongest profile story; dotbot is the most minimal; dotter sits between.

    dodot's distinguishing claim, restated: the principles in §2 hold _all the way through_ the advanced surface. Templates, secrets, plists, conditional running, install scripts — all integrated without an apply step, without git wrapping, without a renamed source tree, without a database, and without growing the command set past three for routine use.
