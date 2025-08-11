// Package homebrew provides the BrewPowerUp implementation for dodot.
//
// # Overview
//
// The BrewPowerUp processes Brewfiles to install packages via Homebrew on macOS and Linux.
// It provides a declarative way to manage system packages, GUI applications (casks),
// and Mac App Store apps as part of your dotfiles setup.
//
// # When It Runs
//
// - **Deploy Mode**: NO - Does not run during `dodot deploy`
// - **Install Mode**: YES - Runs during `dodot install` (RunModeOnce)
// - **Idempotent**: YES - Uses checksums to track Brewfile changes
//
// # Standard Configuration
//
// In your global dodot configuration (~/.config/dodot/config.toml):
//
//	[matchers.brewfiles]
//	trigger = "filename"
//	patterns = ["Brewfile"]
//	powerup = "brew"
//	priority = 95
//
// Or in a pack-specific .dodot.toml:
//
//	[[matchers]]
//	trigger = "filename"
//	patterns = ["Brewfile", "Brewfile.local"]
//	powerup = "brew"
//
// # File Selection Process
//
// 1. **Pack Discovery**: dodot finds all subdirectories in $DOTFILES_ROOT
// 2. **File Walking**: Recursively walks each pack directory
// 3. **Trigger Matching**: FileNameTrigger matches files named "Brewfile"
// 4. **PowerUp Invocation**: Matched files are passed to BrewPowerUp.Process()
//
// Example file structure:
//
//	~/dotfiles/
//	├── base/
//	│   └── Brewfile        # Core tools: git, vim, tmux
//	├── dev/
//	│   └── Brewfile        # Development: node, go, docker
//	└── apps/
//	    └── Brewfile        # GUI apps: vscode, firefox
//
// # Execution Strategy
//
// dodot uses Homebrew's native `brew bundle` command with checksum tracking:
//
// 1. **First Run**: Executes `brew bundle`, stores Brewfile's SHA256 checksum
// 2. **Subsequent Runs**: Compares current checksum with stored value
// 3. **Brewfile Modified**: If checksum differs, runs `brew bundle` again
// 4. **Force Mode**: `--force` flag bypasses checks and re-runs bundle
//
// The actual package management is delegated entirely to Homebrew:
//
//	brew bundle --file="/path/to/Brewfile"
//
// # Storage Locations
//
// - **Sentinel files**: ~/.local/share/dodot/homebrew/<pack_name>
// - **No package cache**: Homebrew manages its own package state
// - **No intermediate files**: Brewfile used directly from source
// - **Homebrew data**: /usr/local (Intel) or /opt/homebrew (Apple Silicon)
//
// # Environment Variable Tracking
//
// Completed Brewfile installations are tracked via the `DODOT_BREWFILES` environment
// variable when dodot-init.sh is sourced:
//
// - **Variable**: `DODOT_BREWFILES`
// - **Format**: Colon-separated list of pack names with processed Brewfiles
// - **Example**: `base:dev:apps:tools`
// - **Usage**: `echo $DODOT_BREWFILES | tr ':' '\n'` to list completed packs
//
// This helps track which packs have had their Brewfiles successfully processed.
//
// # Brewfile Syntax
//
// BrewPowerUp supports all standard Brewfile commands:
//
//	# Taps (third-party repositories)
//	tap "homebrew/cask-fonts"
//
//	# CLI tools
//	brew "git"
//	brew "node@18"
//	brew "ripgrep"
//
//	# GUI applications (macOS only)
//	cask "visual-studio-code"
//	cask "firefox"
//
//	# Mac App Store apps (macOS only, requires mas)
//	mas "Xcode", id: 497799835
//
//	# With options
//	brew "macvim", args: ["with-override-system-vim"]
//
// # Effects on User Environment
//
// - **Installs**: System packages, GUI applications, fonts
// - **Creates**: Symlinks in /usr/local/bin or /opt/homebrew/bin
// - **Modifies**: PATH (via Homebrew's standard setup)
// - **Downloads**: Package files to Homebrew's cache
// - **Reversible**: Yes, via `brew uninstall` or `brew bundle cleanup`
//
// # Options
//
// Currently, BrewPowerUp accepts no configuration options.
// All configuration is done through Brewfile content.
//
// # Brewfile Template
//
// Use `dodot new brew <pack>` to create a Brewfile from template:
//
//	# Homebrew Bundle file for <pack>
//	# https://github.com/Homebrew/homebrew-bundle
//
//	# Taps
//	# tap "homebrew/cask-fonts"
//
//	# Packages
//	# brew "git"
//	# brew "vim"
//
//	# Casks (macOS apps)
//	# cask "visual-studio-code"
//
//	# Mac App Store
//	# mas "Xcode", id: 497799835
//
// # Example End-to-End Flow
//
// User runs: `dodot install`
//
// 1. dodot finds ~/dotfiles/dev/Brewfile
// 2. FileNameTrigger matches "Brewfile" pattern
// 3. BrewPowerUp checks sentinel file at ~/.local/share/dodot/homebrew/dev
// 4. No sentinel or checksum mismatch: proceeds with execution
// 5. Generates action with command: `brew bundle --file="~/dotfiles/dev/Brewfile"`
// 6. Executor runs the command with 5-minute timeout
// 7. Homebrew installs/updates packages defined in Brewfile
// 8. On success, stores Brewfile's SHA256 checksum in sentinel
// 9. Future runs skip unless Brewfile modified or --force used
//
// # Error Handling
//
// Common errors and their codes:
//
// - **BREW001**: Homebrew not installed
// - **BREW002**: Brewfile not found
// - **BREW003**: Bundle execution failed
// - **BREW004**: Timeout exceeded (5 minutes)
// - **BREW005**: Network error downloading packages
//
// # Best Practices
//
// 1. **Organize by purpose**: Separate Brewfiles for different roles (base, dev, apps)
// 2. **Pin versions**: Use `brew "node@18"` for consistency across machines
// 3. **Document packages**: Add comments explaining why each package is needed
// 4. **Test incrementally**: Add a few packages at a time
// 5. **Use brew bundle cleanup**: Remove packages not in Brewfile
// 6. **Avoid conflicts**: Don't install the same package in multiple Brewfiles
// 7. **Check platform**: Use conditionals for macOS-specific casks
//
// # Platform Considerations
//
// - **macOS**: Full support for brew, cask, and mas commands
// - **Linux**: Only brew commands supported (no cask or mas)
// - **Homebrew Path**: Automatically detected (/usr/local or /opt/homebrew)
// - **Prerequisites**: Homebrew must be installed before using dodot
//
// # Comparison with Other PowerUps
//
// - **InstallScriptPowerUp**: For complex installations Homebrew can't handle
// - **BrewPowerUp**: For system packages and macOS applications
// - **SymlinkPowerUp**: For configuration files (deploy phase)
// - **ProfilePowerUp**: For shell configuration (deploy phase)
//
// Use BrewPowerUp for system-level package management, especially when you want
// a declarative, version-controlled approach to managing your development tools
// and applications across multiple machines.
package homebrew
