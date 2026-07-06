#!/usr/bin/env bash
# Interactive exploration environment for dodot.
#
# Sets up a realistic sandbox with pre-existing home files,
# so you can immediately experiment with all commands:
#
#   dodot status          — see pending packs
#   dodot up              — deploy (hits conflicts on pre-existing files)
#   dodot up --dry-run    — preview what would happen
#   dodot adopt vim ~/.vimrc  — adopt an existing file into a pack
#   dodot down            — tear down
#
# The sandbox is at $HOME. DOTFILES_ROOT points to ~/dotfiles.
# Everything is isolated — nothing escapes the container.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export DODOT_BIN="${DODOT_BIN:-/usr/local/bin/dodot}"

# Source helpers
# shellcheck source=/dev/null
source "$SCRIPT_DIR/helpers/setup.bash"

# ── Build sandbox ───────────────────────────────────────────────

sandbox_setup

# Build the full dotfiles repo
create_realistic_dotfiles

# Also add a dot. prefix pack for testing that convention
instrumented_shell "shell" "dot.bashrc" 'alias reload="source ~/.bashrc"'
instrumented_shell "shell" "dot.zshrc" 'alias reload="source ~/.zshrc"'
create_pack_file "shell" "dot.inputrc" 'set editing-mode vi'

# Install the brew mock so homebrew handler works
install_brew_mock

# ── Pre-populate HOME (simulating a real system) ────────────────

# Existing dotfiles that will CONFLICT with dodot up
# shellcheck disable=SC2016  # literal $PATH — expanded when the fixture .bashrc is sourced, not here
create_home_file ".bashrc" '# System bashrc — pre-existing
export PATH="/usr/local/bin:$PATH"
echo "Welcome to the explore sandbox!"
'

create_home_file ".gitconfig" '[user]
    name = Explorer
    email = explorer@example.com
[core]
    editor = vim
'

# Existing config files — will conflict with nvim pack
mkdir -p "$HOME/.config/nvim"
cat >"$HOME/.config/nvim/init.lua" <<'LUA'
-- Pre-existing nvim config (will conflict with nvim pack)
vim.opt.number = true
vim.opt.relativenumber = true
LUA

# Existing ssh config — will conflict with ssh pack
mkdir -p "$HOME/.ssh"
cat >"$HOME/.ssh/config" <<'SSH'
# Pre-existing SSH config
Host github.com
    IdentityFile ~/.ssh/id_ed25519
SSH

# Files that are good candidates for `dodot adopt`
create_home_file ".tmux.conf" 'set -g mouse on
set -g default-terminal "screen-256color"
'

create_home_file ".config/starship.toml" '[character]
success_symbol = "[➜](bold green)"
error_symbol = "[✗](bold red)"
'

create_home_file ".config/alacritty/alacritty.toml" '[font]
size = 14.0
[font.normal]
family = "JetBrains Mono"
'

# ── Print welcome ───────────────────────────────────────────────

cat <<'WELCOME'

┌─────────────────────────────────────────────────┐
│           dodot explore sandbox                  │
├─────────────────────────────────────────────────┤
│                                                  │
│  Dotfiles repo:  ~/dotfiles                      │
│  Home dir:       ~ (sandboxed)                   │
│                                                  │
│  Packs available:                                │
│    vim/    git/    zsh/    nvim/    tools/        │
│    ssh/    shell/                                 │
│    disabled/ (ignored — for testing .dodotignore) │
│                                                  │
│  Pre-existing files (will cause conflicts):      │
│    ~/.bashrc  ~/.gitconfig  ~/.ssh/config         │
│    ~/.config/nvim/init.lua                        │
│                                                  │
│  Adoptable files (no pack yet):                  │
│    ~/.tmux.conf  ~/.config/starship.toml          │
│    ~/.config/alacritty/alacritty.toml             │
│                                                  │
│  Try:                                            │
│    dodot status                                  │
│    dodot up --dry-run                            │
│    dodot init tmux &&                            │
│    dodot adopt tmux ~/.tmux.conf                 │
│    dodot init mypack                             │
│    eval "$(dodot init-sh)"                       │
│                                                  │
│  Helpers available:                              │
│    source /tests/helpers/setup.bash              │
│    create_pack_file, assert_symlink, etc.        │
│                                                  │
└─────────────────────────────────────────────────┘

WELCOME

# Show current state
echo "Current status:"
echo ""
dodot status 2>&1 | head -30
echo ""
echo "---"
echo ""

# Drop into interactive shell (only when stdin is a terminal)
if [ -t 0 ]; then
	export PS1="[dodot-explore] \w \$ "
	cd "$HOME"
	exec bash --norc --noprofile -i
fi
