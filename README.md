# dodot

A dotfiles manager that uses symlinks for live editing. Your dotfiles repository IS your live configuration -- edit either the original or the symlinked file, changes apply immediately.

## Why dodot?

- **No apply step.** Symlinks mean edits are always live.
- **No database.** Git is the source of truth; the filesystem is the state.
- **Minimal structure.** Organize your dotfiles in directories, name files by convention, done.
- **Transparent.** `dodot status` shows what will happen before it does.

## Quick Start

```sh
# Install (Homebrew)
brew install arthur-debert/tap/dodot

# Or from source
cargo install dodot

# Go to your dotfiles repo
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
    aliases.sh  ⚙ shell profile             pending
    bin         + $PATH/bin                  pending
    init.lua    ➞ ~/.config/nvim/init.lua   pending
    lua         ➞ ~/.config/nvim/lua        pending
    Brewfile    ⚙ brew install               pending
```

Pack-root entries default to `$XDG_CONFIG_HOME/<pack>/<name>` — the pack name namespaces config under XDG, matching the convention modern tools (nvim, helix, ghostty, kitty, …) follow. Files like `~/.bashrc` that legacy tools expect in `$HOME` go through `force_home` (auto-handled for canonical names) or the per-file `home.X` opt-in prefix.

Preview with `dodot up --dry-run`, then deploy:

```sh
$ dodot up nvim

Packs deployed.
nvim
    init.lua    ➞ ~/.config/nvim/init.lua   deployed
    lua         ➞ ~/.config/nvim/lua        deployed
    aliases.sh  ⚙ shell profile              sourced
    Brewfile    ⚙ brew install                installed
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
| **shell**  | `{aliases,profile,login,env}.{sh,bash,zsh}` | Sourced via shell init script |
| **path**   | `bin/` directories                          | Added to `$PATH`              |
| **homebrew** | `Brewfile`                                | `brew bundle install`         |
| **install**| `install.sh`, `install.bash`, `install.zsh` | Run once (checksum-tracked)   |

Symlink targets are resolved smartly:
- Pack-root entries default to `$XDG_CONFIG_HOME/<pack>/<rel_path>` (e.g. `nvim/init.lua` → `~/.config/nvim/init.lua`, `warp/themes/` → `~/.config/warp/themes/`)
- `force_home` blacklist routes canonical legacy tools to `$HOME/.<name>` regardless (ssh, gpg, bashrc, zshrc, etc.)
- Per-file `home.X` prefix opts a single file into `$HOME/.X` placement (e.g. `git/home.gitconfig` → `~/.gitconfig`)
- Per-subtree `_home/` and `_xdg/` directory prefixes route whole groups of files to `$HOME` or `$XDG_CONFIG_HOME` respectively, skipping the pack-name namespace
- Per-file `[symlink.targets]` config maps any pack file to any absolute or XDG-relative path

dodot uses a double-symlink architecture (`~/.config/nvim/init.lua → datastore/... → ~/dotfiles/...`) for clean state management. Edits still flow through both links instantly.

All conventions can be overridden via `.dodot.toml` in the pack or the dotfiles root.

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

### Pre-built binaries

Download from [GitHub Releases](https://github.com/arthur-debert/dodot/releases) for macOS (ARM) and Linux (x86_64, ARM).

### Cargo

```sh
cargo install dodot
```

### From source

```sh
git clone https://github.com/arthur-debert/dodot.git
cd dodot
cargo build --release
# Binary at target/release/dodot
```

## Development

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

## License

MIT
