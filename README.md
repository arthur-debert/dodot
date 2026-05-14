# dodot

A dotfiles manager designed to adapt to you, rather than the other way around.

- **Minimal structure.** Group files into directories ("packs"); special filenames map to special actions.
- **No mapping required**, unless you want to override the conventions.
- **No apply step, no database.** Symlinks mean edits are always live; git is the source of truth, the filesystem is the state.
- **No tooling changes.** Edit your files however you already do; `git` owns source control end-to-end.
- **Three commands cover daily use:** `status`, `up`, `down`.
- **Correct path handling**, including the macOS-specific awkward bits (XDG vs. `~/.config`, Application Support, plists).

While still featuring the advanced bits when you need them:

- **Shell integration** for sourcing files and managing `$PATH`.
- **OS-level provisioning** via `Brewfile` and `install.sh`.
- **Templates** for per-host config (`.tmpl` / `.template`, Jinja2-style, rendered at deploy time).
- **Secrets** injected from `pass`, 1Password (`op`), Bitwarden (`bw`), `sops`, `gpg`, `age`, the macOS Keychain, or freedesktop Secret Service — plus whole-file `.age` / `.gpg` decryption.
- **Conditional deployment** gated by OS, architecture, or hostname.
- **macOS plists** brought under normal `git diff` review via clean/smudge filters.

## Quick Start

```sh
# Install (Homebrew)
brew install arthur-debert/tools/dodot

# Or from crates.io
cargo install dodot

# Go to your dotfiles repo (or set $DOTFILES_ROOT)
cd ~/dotfiles

# See what dodot will do
dodot status

# Deploy everything (or specific packs)
dodot up
dodot up git nvim

# Shell integration (add to .zshrc / .bashrc)
eval "$(dodot init-sh)"

# Remove deployments cleanly
dodot down git
```

First time on a repo? `dodot tutorial` walks you through deploying one pack interactively, using your real files, in about ten minutes — nothing changes without an explicit yes.

## How It Works

dodot discovers directories in your dotfiles root as **packs** and uses file naming conventions to decide what to do with each file:

```
nvim/
+-- Brewfile    -> homebrew installs neovim, ripgrep, fd
+-- aliases.sh  -> sourced by shell
+-- bin/        -> added to PATH
+-- init.lua    -> symlinked to ~/.config/nvim/init.lua
+-- lua/        -> symlinked wholesale to ~/.config/nvim/lua
```

```sh
$ dodot status nvim

nvim
    aliases.sh  ⚙ shell profile               pending
    bin         + $PATH/bin                   pending
    init.lua    ➞ ~/.config/nvim/init.lua     pending
    lua         ➞ ~/.config/nvim/lua          pending
    Brewfile    ⚙ brew install                pending
```

Pack-root entries default to `$XDG_CONFIG_HOME/<pack>/<name>` — the pack name namespaces config under XDG, matching the convention modern tools (nvim, helix, ghostty, kitty, …) follow. Files like `~/.bashrc` that legacy tools expect in `$HOME` go through `force_home` (auto-handled for canonical names) or the per-file `home.X` opt-in prefix.

Preview with `dodot up --dry-run`, then deploy:

```sh
$ dodot up nvim

Packs deployed.
nvim
    init.lua    ➞ ~/.config/nvim/init.lua    deployed
    lua         ➞ ~/.config/nvim/lua         deployed
    aliases.sh  ⚙ shell profile               sourced
    Brewfile    ⚙ brew install              installed
```

Edit your config -- changes are immediate:

```sh
vim ~/.gitconfig        # same file as ~/dotfiles/git/gitconfig
```

## Handlers

dodot matches files to handlers by name convention:

| Handler    | Matches                                     | Action                        |
|------------|---------------------------------------------|-------------------------------|
| **symlink**| Most files (default)                        | Symlink under `~/.config/<pack>/` |
| **shell**  | `*.{sh,bash,zsh}` at pack root              | Sourced via shell init script |
| **path**   | `bin/` directories                          | Added to `$PATH`              |
| **homebrew** | `Brewfile`                                | `brew bundle install`; edits report `older version`, apply with `dodot up --force` |
| **install**| `install.sh`, `install.bash`, `install.zsh` | Run once (checksum-tracked); edits report `older version`, apply with `dodot up --force` |

Symlink targets are resolved smartly:
- Pack-root entries default to `$XDG_CONFIG_HOME/<pack>/<rel_path>` (e.g. `nvim/init.lua` → `~/.config/nvim/init.lua`, `warp/themes/` → `~/.config/warp/themes/`)
- `force_home` blacklist routes canonical legacy tools to `$HOME/.<name>` regardless (ssh, gpg, bashrc, zshrc, etc.)
- Per-file `home.X` prefix opts a single file into `$HOME/.X` placement (e.g. `git/home.gitconfig` → `~/.gitconfig`)
- Per-subtree `_home/` and `_xdg/` directory prefixes route whole groups of files to `$HOME` or `$XDG_CONFIG_HOME` respectively, skipping the pack-name namespace
- Per-file `[symlink.targets]` config maps any pack file to any absolute or XDG-relative path

dodot uses a double-symlink architecture (`~/.config/nvim/init.lua → datastore/... → ~/dotfiles/...`) for clean state management. Edits still flow through both links instantly.

All conventions can be overridden via `.dodot.toml` in the pack or the dotfiles root.

## macOS Plists

dodot brings macOS GUI app preferences (binary `*.plist` files at `~/Library/Preferences/...` and `~/Library/Application Support/<App>/...`) under the same review/diff/cherry-pick workflow as plain-text dotfiles, by translating them through git clean/smudge filters:

- The file in your pack is the binary the app reads.
- What git stores is canonical, alphabetically-sorted XML.
- `git status` and `git diff` show settings changes the way they show every other dotfile change.

Setup is a one-liner per machine plus a single `.gitattributes` line in the repo:

```sh
$ dodot git-install-filters
$ echo '*.plist filter=dodot-plist' >> .gitattributes
$ git add .gitattributes && git commit -m 'enable plist filters'
```

Adopt existing settings with `dodot adopt --into <pack> ~/Library/Preferences/com.app.plist`. The full reference is at `docs/reference/plists.lex`. macOS-only at deploy time; the CLI surface (`dodot plist clean/smudge`) is platform-agnostic.

## Commands

| Command      | Description                                      |
|--------------|--------------------------------------------------|
| `up`         | Deploy packs (symlinks, shell, installs)         |
| `down`       | Remove all deployments for packs                 |
| `status`     | Show what dodot will do / has done               |
| `list`       | List all discovered packs                        |
| `init`       | Create a new pack with template files            |
| `adopt`      | Move existing files into a pack, symlink back    |
| `fill`       | Add template files to an existing pack           |
| `addignore`  | Drop a `.dodotignore` marker (pack-ignore)       |
| `init-sh`    | Print shell init script for `eval`               |
| `config`     | Inspect and modify configuration                 |
| `plist`      | clean/smudge filters for macOS plists (stdin→stdout) |
| `git-install-filters` | Wire plist filters into the repo's `.git/config`  |
| `git-show-filters`    | Print plist filter config snippets without writing |
| `prompts`    | Inspect/reset dismissed one-time prompts         |

All commands accept pack names as arguments (`dodot up git nvim`) or operate on all packs when run without arguments.

## Configuration

dodot uses a layered TOML configuration:

1. **Built-in defaults** -- sensible conventions for most setups
2. **Root `.dodot.toml`** -- overrides applied to all packs
3. **Pack `.dodot.toml`** -- overrides for a specific pack

```toml
# git/.dodot.toml
[mappings]
shell = ["aliases.sh", "profile.sh"]
ignore = ["scratch.txt"]   # drop entirely (silent, like .gitignore)
skip   = []                # clear defaults to deploy README/LICENSE
```

`mappings.ignore` matches files dodot should drop without a trace; `mappings.skip` matches files that show up in `dodot status` as `skipped` but aren't deployed (defaults cover `README`, `LICENSE`, `CHANGELOG`, and friends).

Generate a starter config with `dodot config gen -o .dodot.toml`.

## Installation

### Homebrew (macOS, Linux)

```sh
brew install arthur-debert/tools/dodot
```

### crates.io

```sh
cargo install dodot
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/arthur-debert/dodot/releases):

- macOS (Apple Silicon): `dodot-aarch64-apple-darwin.tar.gz`
- Linux x86_64: `dodot-x86_64-unknown-linux-gnu.tar.gz`
- Linux ARM: `dodot-aarch64-unknown-linux-gnu.tar.gz`
- Debian/Ubuntu: `dodot_<version>-1_amd64.deb`, `dodot_<version>-1_arm64.deb`

## Learn More

- `dodot tutorial` — interactive walkthrough on your real dotfiles
- `dodot --help` and `dodot help <command>` — full CLI reference
- [`docs/user/`](docs/user/) — getting started, command-by-command, handlers, templates, secrets, conditional running

## Contributing

```sh
# Run all checks (same as CI)
scripts/check

# Individual checks
scripts/check-fmt
scripts/check-lint
scripts/check-tests

# Install pre-commit hook (requires lefthook: `brew install lefthook`)
lefthook install
```

Issues and PRs welcome at [github.com/arthur-debert/dodot](https://github.com/arthur-debert/dodot).

## License

MIT
