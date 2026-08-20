#!/usr/bin/env bash
# Fixture creation helpers — shell equivalent of TempEnvironment builder.
#
# All paths are relative to the current sandbox's DOTFILES_ROOT and HOME.
#
# ## Instrumented fixtures
#
# Functions prefixed with `instrumented_` create self-reporting fixtures.
# When loaded/executed, they set detectable markers:
#
#   Shell files:  export DODOT_LOADED_{PACK}_{FILE}=1   (env var)
#   Bin scripts:  echo DODOT_BIN_{PACK}_{SCRIPT}        (stdout)
#   Install:      write to $HOME/.dodot-markers/{pack}.install   (file)
#   Brew mock:    log args to $HOME/.dodot-markers/brew.log
#
# The naming convention normalizes pack/file names:
#   "vim" + "aliases.sh" → DODOT_LOADED_VIM_ALIASES_SH

# Create a pack directory.
# Usage: create_pack "vim"
create_pack() {
	local pack="$1"
	mkdir -p "$DOTFILES_ROOT/$pack"
}

# Create a file inside a pack.
# Usage: create_pack_file "vim" "home.vimrc" "set nocompatible"
# Handles nested paths (creates parent dirs).
create_pack_file() {
	local pack="$1"
	local rel_path="$2"
	local contents="${3:-}"
	local full_path="$DOTFILES_ROOT/$pack/$rel_path"

	mkdir -p "$(dirname "$full_path")"
	printf '%b' "$contents" >"$full_path"
}

# Write a .dodot.toml config for a specific pack.
# Usage: create_pack_config "vim" '[pack]\nignore = ["*.bak"]'
create_pack_config() {
	local pack="$1"
	local toml="$2"

	printf '%b' "$toml" >"$DOTFILES_ROOT/$pack/.dodot.toml"
}

# Write a .dodot.toml at the dotfiles root.
# Usage: create_root_config '[symlink]\nstrip_dot_prefix = true'
create_root_config() {
	local toml="$1"
	printf '%b' "$toml" >"$DOTFILES_ROOT/.dodot.toml"
}

# Mark a pack as ignored by creating .dodotignore.
# Usage: mark_ignored "disabled-pack"
mark_ignored() {
	local pack="$1"
	mkdir -p "$DOTFILES_ROOT/$pack"
	touch "$DOTFILES_ROOT/$pack/.dodotignore"
}

# Create a file under the simulated HOME directory.
# Useful for testing adopt.
# Usage: create_home_file ".vimrc" "set nocompatible"
create_home_file() {
	local rel_path="$1"
	local contents="${2:-}"
	local full_path="$HOME/$rel_path"

	mkdir -p "$(dirname "$full_path")"
	printf '%b' "$contents" >"$full_path"
}

# Create an executable script inside a pack.
# Usage: create_pack_script "tools" "install.sh" '#!/bin/sh
# echo "installed" > "$HOME/.tools-installed"'
create_pack_script() {
	local pack="$1"
	local rel_path="$2"
	local contents="$3"

	create_pack_file "$pack" "$rel_path" "$contents"
	chmod +x "$DOTFILES_ROOT/$pack/$rel_path"
}

# Create a bin directory inside a pack with executable scripts.
# Usage: create_pack_bin "tools" "myscript" '#!/bin/sh
# echo hello'
create_pack_bin() {
	local pack="$1"
	local script_name="$2"
	local contents="$3"

	create_pack_file "$pack" "bin/$script_name" "$contents"
	chmod +x "$DOTFILES_ROOT/$pack/bin/$script_name"
}

# ── Naming convention ───────────────────────────────────────────

# Normalize a string to an env-var-safe identifier.
# Replaces dots, hyphens, slashes with underscores, uppercases.
# Usage: _normalize "vim" "aliases.sh"  →  VIM_ALIASES_SH
_normalize() {
	local result=""
	for part in "$@"; do
		[[ -n "$result" ]] && result="${result}_"
		result="${result}${part}"
	done
	echo "$result" | tr '[:lower:]' '[:upper:]' | tr './-' '___'
}

# ── Instrumented fixture generators ─────────────────────────────

# Create an instrumented shell file that exports a marker env var when sourced.
# Usage: instrumented_shell "vim" "aliases.sh"
# Creates: dotfiles/vim/aliases.sh containing:
#   export DODOT_LOADED_VIM_ALIASES_SH=1
# Optionally append extra content:
#   instrumented_shell "vim" "aliases.sh" 'alias vi=vim'
instrumented_shell() {
	local pack="$1"
	local filename="$2"
	local extra="${3:-}"
	local var_name
	var_name="DODOT_LOADED_$(_normalize "$pack" "$filename")"

	local contents="export ${var_name}=1"
	[[ -n "$extra" ]] && contents="${contents}
${extra}"

	create_pack_file "$pack" "$filename" "$contents"
}

# Create an instrumented bin script that echoes a marker when run.
# Usage: instrumented_bin "tools" "mytool"
# Creates: dotfiles/tools/bin/mytool containing:
#   #!/bin/sh
#   echo DODOT_BIN_TOOLS_MYTOOL
# Optionally append extra content:
#   instrumented_bin "tools" "mytool" 'echo "extra output"'
instrumented_bin() {
	local pack="$1"
	local script_name="$2"
	local extra="${3:-}"
	local marker
	marker="DODOT_BIN_$(_normalize "$pack" "$script_name")"

	local contents="#!/bin/sh
echo ${marker}"
	[[ -n "$extra" ]] && contents="${contents}
${extra}"

	create_pack_bin "$pack" "$script_name" "$contents"
}

# Create an instrumented install script that writes a marker file.
# Usage: instrumented_install "tools"
# Creates: dotfiles/tools/install.sh that writes to:
#   $HOME/.dodot-markers/tools.install
# Optionally append extra content:
#   instrumented_install "tools" 'apt-get update'
instrumented_install() {
	local pack="$1"
	local extra="${2:-}"

	# shellcheck disable=SC2016  # literal $HOME — expanded when the generated install.sh runs, not here
	local contents='#!/bin/sh
mkdir -p "$HOME/.dodot-markers"
echo "executed" > "$HOME/.dodot-markers/'"${pack}"'.install"'
	[[ -n "$extra" ]] && contents="${contents}
${extra}"

	create_pack_script "$pack" "install.sh" "$contents"
}

# Create a Brewfile in a pack.
# Usage: instrumented_brewfile "dev"
# Creates: dotfiles/dev/Brewfile with tap/brew lines.
# The actual brew command will be intercepted by the brew mock.
instrumented_brewfile() {
	local pack="$1"
	local contents="${2:-brew \"ripgrep\"}"

	create_pack_file "$pack" "Brewfile" "$contents"
}

# Install a mock `brew` script that logs every `brew bundle` run.
# Must be called during setup before `dodot up`.
# Usage: install_brew_mock
# Creates: $HOME/.dodot-markers/brew-mock/bin/brew
# Logs to: $HOME/.dodot-markers/brew.log   (arguments, one line per
#          `brew bundle`)
#          $HOME/.dodot-markers/brew-env.log (the HOMEBREW_* environment
#          each call was spawned with, so a test can check what dodot set
#          rather than only what it passed on the command line). Each row
#          ends with `:: brew <argv>`, so an assertion can name which
#          invocation it means once dodot runs brew more than once.
#
# Only `bundle` reaches brew.log, because the provisioning handler is
# not the only caller: naming a prefix hands the same brew to the
# shell bootstrap, which runs `brew shellenv <shell>` on every
# `dodot up` to capture the block it writes into the init script.
# Logging that too would make `assert_brew_not_invoked` — "the
# Brewfile did not run" — fail on a run where the Brewfile was
# correctly skipped. The env log stays unfiltered, since its rows are
# already scoped by the argv that produced them. The mock answers
# `shellenv` with nothing, so the bootstrap records a brew that did
# not answer and writes no Homebrew block, which is the right outcome
# for a fixture that is not a real Homebrew.
#
# The mock is announced through $HOMEBREW_PREFIX rather than through
# PATH. dodot locates a provisioner by testing a fixed list of
# absolute candidates and spawns the path that answered (ADR-0007);
# $HOMEBREW_PREFIX leads that list, so this both points dodot at the
# mock and outranks any real brew on the host — a GitHub Linux runner
# ships one at /home/linuxbrew/.linuxbrew/bin/brew, and a PATH-only
# mock would leave `dodot up` running it for real.
#
# Leaving PATH alone is also what makes these tests a regression
# guard: if dodot went back to resolving `brew` through PATH it would
# find the sandbox's non-zero brew muzzle, the log would stay empty,
# and `assert_brew_invoked` would fail.
install_brew_mock() {
	local prefix="$HOME/.dodot-markers/brew-mock"
	mkdir -p "$prefix/bin"

	cat >"$prefix/bin/brew" <<'MOCK'
#!/bin/sh
mkdir -p "$HOME/.dodot-markers"
echo "HOMEBREW_NO_AUTO_UPDATE=${HOMEBREW_NO_AUTO_UPDATE-<unset>} :: brew $*" >> "$HOME/.dodot-markers/brew-env.log"
if [ "$1" = "bundle" ]; then
	echo "$@" >> "$HOME/.dodot-markers/brew.log"
fi
MOCK
	chmod +x "$prefix/bin/brew"

	export HOMEBREW_PREFIX="$prefix"
}

# ── Eval helper ─────────────────────────────────────────────────

# Eval dodot init-sh in the current shell and return the output.
# After calling this, env vars from shell files and PATH changes are live.
# Usage: eval_init_sh
eval_init_sh() {
	eval "$("$DODOT_BIN" init-sh)"
}

# ── Realistic dotfiles fixture ──────────────────────────────────

# Build a complete multi-pack dotfiles repo exercising all handlers.
# Usage: create_realistic_dotfiles
#
# Packs created:
#   vim/      - symlink (vimrc), shell (aliases.sh)
#   git/      - symlink (gitconfig)
#   zsh/      - shell (aliases.sh, profile.sh, login.sh)
#   nvim/     - symlink (init.lua + lua/plugins.lua) → ~/.config/nvim/...
#   tools/    - install (install.sh), path (bin/devtool), Brewfile
#   ssh/      - symlink with force_home (ssh/config) — tests force_home routing
#   disabled/ - ignored pack
#
# Also creates a minimal root .dodot.toml so the config file is loadable.
create_realistic_dotfiles() {
	# vim: symlink + shell
	instrumented_shell "vim" "aliases.sh" 'alias vi=vim'
	create_pack_file "vim" "home.vimrc" "set nocompatible"

	# git: symlink
	create_pack_file "git" "home.gitconfig" "[user]\n  name = testuser\n  email = test@example.com"

	# zsh: all three shell file types
	instrumented_shell "zsh" "aliases.sh" 'alias ll="ls -la"'
	instrumented_shell "zsh" "profile.sh" 'export ZSH_PROFILE_LOADED=1'
	instrumented_shell "zsh" "login.sh" 'export ZSH_LOGIN_LOADED=1'

	# nvim: top-level file + nested dir route under the pack's XDG dir
	# (post-#48: pack name namespaces — no need for `nvim/nvim/` doubling).
	create_pack_file "nvim" "init.lua" '-- nvim config\nrequire("plugins")'
	create_pack_file "nvim" "lua/plugins.lua" '-- plugin list\nreturn {}'

	# tools: install + path + brew
	instrumented_install "tools"
	instrumented_bin "tools" "devtool"
	instrumented_brewfile "tools"

	# ssh: tests force_home routing (ssh is in default force_home list)
	create_pack_file "ssh" "ssh/config" "Host *\n  ServerAliveInterval 60"

	# disabled: ignored pack
	create_pack_file "disabled" "notes.txt" "scratch"
	mark_ignored "disabled"

	# Root config — keep defaults, just ensure config file is loadable
	create_root_config '[pack]\nignore = []'
}
