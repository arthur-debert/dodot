# dodot

A dotfile manager that works *for* you.
Little to learn, little to configure, brings joy.

## What is dodot?

dodot is a dotfiles manager. Edit your configs as you always have - changes are live, no syncing or rebuilding required.

## Key Features

- **No configuration required** - File naming conventions , (custom mappings if needed)
- **Live editing** - Edit anywhere, changes apply immediately, no workflow
changes.
- **Flexible Organization** - Group your dotfiles in directories any way you
like
- **Git-based** - Your repo structure is the only source of truth.
- **Minimal commands** - Just `up`, `down`, and `status`

## Installation

```bash
brew install arthur-debert/tap/dodot
```

Or download from [releases](https://github.com/arthur-debert/dodot/releases).

## Quick Start

See exactly what dodot will do before making any changes:

```bash
cd ~/dotfiles
dodot status git
```

```bash
git
  gitconfig   ➞  ~/.gitconfig        pending
  aliases.sh  ⚙  source by shell     pending
  bin         +  add to PATH         pending
  Brewfile    📦 brew install        pending
```

dodot shows you what will happen before you run up.
Happy with the plan? Deploy it:

```bash
dodot up git         # Deploy the git pack
git
  gitconfig   ➞  ~/.gitconfig        deployed
  aliases.sh  ⚙  source by shell     deployed
  bin         +  add to PATH         deployed
  Brewfile    📦 brew install        deployed
```

If dodot will do something different than you expect, either rename the file or
change the mappings:
``` bash
    $ dodot gen-config -w git
    $ cat git/.dodot.toml
    [mappings]
    # path = "bin"
    # install = "install.sh"
    # shell = ["aliases.sh", "profile.sh", "login.sh"]
    # homebrew = "Brewfile"
    # ignore = []

```
```

```
Need to disable a pack? Just as easy:
```bash
dodot down git       # Cleanly remove the git pack
# Exclude a directory from dodot
dodot add-ignore <directory_name> # adds a .dodot-ignore file in it
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

**Core Commands:**
- `dodot status [pack...]` - Show current state and pending changes
- `dodot up [pack...]` - Deploy and enable packs
- `dodot down [pack...]` - Remove and disable packs

**Convenience Commands:**
- `dodot init <pack>` - Create a new pack
- `dodot fill <pack>` - Populate a pack with existing dotfiles
- `dodot adopt <pack> <file>` - Move existing file into a pack

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
