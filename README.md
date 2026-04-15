# dodot

A dotfiles manager that uses symlinks for live editing. Your dotfiles repository IS your live configuration -- edit either the original or the symlinked file, changes apply immediately.

## Why dodot?

- **No apply step.** Symlinks mean edits are always live.
- **No database.** Git is the source of truth; the filesystem is the state.
- **Minimal structure.** Organize your dotfiles in directories, name files by convention, done.
- **Transparent.** `ls -la ~` shows exactly what's managed. `dodot status` shows what will happen before it does.

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
git/
+-- Brewfile    -> homebrew installs git, gh, lazygit
+-- alias.sh    -> sourced by shell
+-- bin/        -> added to PATH
+-- gitconfig   -> symlinked to ~/.gitconfig
```

```sh
$ dodot status git

git
    alias.sh    source by shell     pending
    bin         add to PATH         pending
    gitconfig   ~/.gitconfig        pending
    Brewfile    brew install         pending
```

Preview with `--dry-run`, then deploy:

```sh
$ dodot up git
... homebrew:  git/Brewfile: installed
... shell:     git/alias.sh: sourced
... symlink:   git/gitconfig -> ~/.gitconfig: deployed
```

Edit your config -- changes are immediate:

```sh
vim ~/.gitconfig        # same file as ~/dotfiles/git/gitconfig
```

## Handlers

dodot matches files to handlers by name convention:

| Handler    | Matches                          | Action                        |
|------------|----------------------------------|-------------------------------|
| **symlink**| Most files (default)             | Symlink to `~` or `~/.config` |
| **shell**  | `*.sh`, `aliases.*`, `profile.*` | Sourced via shell init script |
| **path**   | `bin/` directories               | Added to `$PATH`              |
| **homebrew** | `Brewfile`                     | `brew bundle install`         |
| **install**| `install.sh`, `install`          | Run once (checksum-tracked)   |

Symlink targets are resolved smartly:
- Top-level files go to `~` with a dot prefix (e.g., `gitconfig` -> `~/.gitconfig`)
- Subdirectories go to `~/.config/` (e.g., `nvim/init.lua` -> `~/.config/nvim/init.lua`)
- Files already starting with `.` keep their name as-is

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
| `genconfig`  | Generate a commented `.dodot.toml` template      |
| `addignore`  | Mark a pack as ignored                           |
| `init-sh`    | Print shell init script for `eval`               |
| `config`     | Inspect resolved configuration                   |

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
ignore = ["README.md"]
```

Generate a starter config with `dodot genconfig --write`.

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

# Install pre-commit hook
ln -sf ../../scripts/pre-commit .git/hooks/pre-commit
```

## License

MIT
