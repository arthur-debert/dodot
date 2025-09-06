# dodot

A dotfile manager that respects your workflow.

## What is dodot?

dodot manages your dotfiles through symlinks and simple conventions. Edit your configs as you always have - changes are live, no syncing or rebuilding required.

## Key Features

- **No configuration required** - File naming conventions handle most cases
- **Live editing** - Edit anywhere, changes apply immediately  
- **Modular packs** - Group related configs, enable/disable together
- **Git-based** - Your repo structure is the only state
- **Minimal commands** - Just `up`, `down`, and `status`

## Installation

```bash
brew install arthur-debert/tap/dodot
```

Or download from [releases](https://github.com/arthur-debert/dodot/releases).

## Quick Start

```bash
cd ~/dotfiles
dodot status          # See what dodot will do
dodot up              # Deploy all packs
dodot down git         # Remove git pack
```

## How It Works

dodot uses simple conventions to manage your dotfiles:

```
dotfiles/
├── git/
│   ├── gitconfig     # Symlinked to ~/.gitconfig
│   ├── aliases.sh    # Sourced in shell profile
│   └── bin/          # Added to PATH
└── vim/
    ├── vimrc         # Symlinked to ~/.vimrc
    └── install.sh    # Run once during setup
```

Each directory is a "pack" that can be enabled or disabled as a unit.

## File Conventions

| Pattern | Action | Example |
|---------|--------|---------|
| `*` | Symlink to home | `vimrc` → `~/.vimrc` |
| `*.sh` | Source in shell | `aliases.sh` sourced on login |
| `bin/` | Add to PATH | `bin/` directory in PATH |
| `install.sh` | Run once | Setup scripts |
| `Brewfile` | Install packages | Homebrew dependencies |

## Commands

- `dodot status [pack...]` - Show current state and pending changes
- `dodot on [pack...]` - Enable packs
- `dodot off [pack...]` - Disable packs
- `dodot init <pack>` - Create a new pack
- `dodot adopt <pack> <file>` - Move existing dotfiles into a pack

Run `dodot --help` for all commands.

## Documentation

- [Getting Started](docs/reference/getting-started.txxt) - Up and running in 5 minutes
- [Commands Reference](docs/reference/commands.txxt) - All commands explained
- [Design Philosophy](docs/reference/design-philosophy.txxt) - Why dodot works this way

## Development

```bash
git clone https://github.com/arthur-debert/dodot
cd dodot
scripts/build
./bin/dodot --version
```

See [Development Guide](docs/dev/development.txxt) for more.

## License

MIT License - see [LICENSE](LICENSE) file.