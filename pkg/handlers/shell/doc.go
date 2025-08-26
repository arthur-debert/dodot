// Package shell provides the ShellProfileHandler implementation for dodot.
//
// # Overview
//
// The ShellProfileHandler manages shell configuration by sourcing shell scripts into
// your shell environment. It provides a non-invasive way to add aliases, functions,
// environment variables, and other shell customizations without directly modifying
// shell configuration files like .bashrc or .zshrc.
//
// # When It Runs
//
// - **Deploy Mode**: YES - Runs during `dodot deploy` (RunModeLinking)
// - **Install Mode**: NO - Does not run during `dodot install`
// - **Idempotent**: YES - Safe to run multiple times
//
// # Standard Configuration
//
// Currently not configured by default. To enable, add to your config:
//
// In your global dodot configuration (~/.config/dodot/config.toml):
//
//	[matchers.shell-profiles]
//	trigger = "filename"
//	patterns = ["aliases.sh", "profile.sh", "*.profile.sh"]
//	handler = "shell"
//	priority = 80
//
// Or in a pack-specific .dodot.toml:
//
//	[[matchers]]
//	trigger = "directory"
//	path = "shell"
//	handler = "shell"
//
// # File Selection Process
//
// 1. **Pack Discovery**: dodot finds all subdirectories in $DOTFILES_ROOT
// 2. **File Walking**: Recursively walks each pack directory
// 3. **Trigger Matching**: FileNameTrigger matches shell scripts (e.g., aliases.sh)
// 4. **Handler Invocation**: Matched files are passed to ShellProfileHandler.Process()
//
// Example file structure:
//
//	~/dotfiles/
//	├── base/
//	│   └── aliases.sh      # Common aliases for all systems
//	├── git/
//	│   └── git-profile.sh  # Git-specific aliases and functions
//	└── dev/
//	    └── dev.profile.sh  # Development environment setup
//
// # Execution Strategy
//
// dodot uses a centralized sourcing approach for shell profiles:
//
// **Step 1 - Registration**:
// - Creates entries in ~/.local/share/dodot/shell/init.sh
// - Each entry sources a matched shell script with existence check
//
// **Step 2 - Shell Integration**:
// - User's shell config sources dodot-init.sh (one-time setup)
// - dodot-init.sh sources all registered scripts on shell startup
//
// **Benefits**:
// - Non-invasive: Only one line added to user's shell config
// - Dynamic: Add/remove scripts without editing .bashrc/.zshrc
// - Organized: All customizations tracked in one place
// - Debuggable: DODOT_SHELL_SOURCE_FLAG tracks sourced files
//
// # Storage Locations
//
// - **Registration file**: ~/.local/share/dodot/shell/init.sh
// - **Deployed scripts**: Referenced from their original pack locations
// - **No copying**: Scripts are sourced directly from dotfiles
// - **No sentinels**: Deploy actions are always idempotent
//
// # Environment Variable Tracking
//
// Sourced shell profiles are tracked via the `DODOT_SHELL_PROFILES` environment
// variable when dodot-init.sh is sourced:
//
// - **Variable**: `DODOT_SHELL_PROFILES`
// - **Format**: Colon-separated list of relative paths from dotfiles root
// - **Example**: `base/aliases.sh:git/git-profile.sh:dev/dev.profile.sh`
// - **Usage**: `echo $DODOT_SHELL_PROFILES | tr ':' '\n'` to list all profiles
//
// This allows you to see which shell scripts are being sourced into your environment.
//
// # Shell Compatibility
//
// The generated source commands work with:
// - **Bash**: Full support
// - **Zsh**: Full support
// - **Fish**: Requires bass (bash compatibility layer)
// - **POSIX sh**: Basic support (no advanced features)
//
// Scripts should use POSIX-compatible syntax when possible for maximum compatibility.
//
// # Effects on User Environment
//
// - **Sources scripts**: On every new shell session
// - **Adds to environment**: Aliases, functions, variables defined in scripts
// - **Modifies PATH**: If scripts export PATH changes
// - **Reversible**: Remove entries from init.sh to stop sourcing
// - **No backups needed**: Original shell configs untouched
//
// # Options
//
// Currently, ShellProfileHandler accepts no configuration options.
// All behavior is controlled by the scripts themselves.
//
// # Script Template
//
// Use `dodot new shell-profile <pack>` to create a script from template:
//
//	#!/usr/bin/env bash
//	# Shell profile for <pack>
//
//	# Aliases
//	alias ll='ls -la'
//	alias gs='git status'
//
//	# Functions
//	mkcd() {
//	    mkdir -p "$1" && cd "$1"
//	}
//
//	# Environment variables
//	export EDITOR=vim
//
//	# Path modifications
//	export PATH="$HOME/.local/bin:$PATH"
//
// # Example End-to-End Flow
//
// User runs: `dodot deploy`
//
//  1. dodot finds ~/dotfiles/git/aliases.sh
//  2. FileNameTrigger matches "aliases.sh" pattern
//  3. ShellProfileHandler creates ActionTypeShellSource action
//  4. DirectExecutor appends to ~/.local/share/dodot/shell/init.sh:
//     ```
//     # Source aliases.sh from git
//     [ -f "/home/user/dotfiles/git/aliases.sh" ] && source "/home/user/dotfiles/git/aliases.sh"
//     ```
//  5. On next shell startup, dodot-init.sh sources init.sh
//  6. Git aliases become available in the shell
//  7. User can edit aliases.sh and changes take effect on next shell
//
// # Error Handling
//
// Shell sourcing errors are handled gracefully:
//
// - **File not found**: Silently skipped (existence check)
// - **Syntax errors**: Reported by shell but don't break startup
// - **Permission errors**: Logged but shell continues
// - **Circular dependencies**: User's responsibility to avoid
//
// # Best Practices
//
// 1. **Use descriptive names**: `git-aliases.sh` better than `aliases.sh`
// 2. **Check dependencies**: Verify commands exist before aliasing
// 3. **Avoid side effects**: Don't run commands, just define them
// 4. **Group by function**: One script per tool/purpose
// 5. **Document complex functions**: Add comments explaining usage
// 6. **Test portability**: Ensure scripts work in bash and zsh
// 7. **Prefix functions**: Avoid naming conflicts (e.g., `git_branch()`)
//
// # Common Use Cases
//
// - **Aliases**: Shortcuts for common commands
// - **Functions**: Complex operations wrapped in functions
// - **Prompts**: Custom PS1/PROMPT configurations
// - **Completions**: Load tool-specific completions
// - **Environment**: Set EDITOR, PAGER, etc.
// - **PATH setup**: Add directories to PATH
// - **Tool initialization**: Load nvm, rbenv, etc.
//
// # Integration with dodot-init.sh
//
// The one-time setup adds to your shell config:
//
//	# For bash (~/.bashrc)
//	[ -f "$HOME/.local/share/dodot/init/dodot-init.sh" ] && source "$HOME/.local/share/dodot/init/dodot-init.sh"
//
//	# For zsh (~/.zshrc)
//	[ -f "$HOME/.local/share/dodot/init/dodot-init.sh" ] && source "$HOME/.local/share/dodot/init/dodot-init.sh"
//
//	# For fish (~/.config/fish/config.fish)
//	[ -f "$HOME/.local/share/dodot/init/dodot-init.fish" ] && source "$HOME/.local/share/dodot/init/dodot-init.fish"
//
// # Comparison with Other Handlers
//
// - **SymlinkHandler**: For static config files
// - **PathHandler**: Specifically for PATH modifications
// - **ShellProfileHandler**: For dynamic shell environment setup
// - **InstallScriptHandler**: For one-time setup tasks
//
// Use ShellProfileHandler when you need to add dynamic behavior to your shell
// environment, such as aliases, functions, or conditional configuration that
// varies by machine or environment.
package shell
