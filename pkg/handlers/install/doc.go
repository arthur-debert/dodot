// Package install provides the InstallScriptHandler implementation for dodot.
//
// # Overview
//
// The InstallScriptHandler executes shell scripts for one-time setup tasks that should
// only run once per pack. This is ideal for installing dependencies, downloading resources,
// creating directories, or performing initial configuration that doesn't need to be repeated.
//
// # When It Runs
//
// - **Link Mode**: NO - Does not run during `dodot link`
// - **Provision Mode**: YES - Runs during `dodot provision` (RunModeProvisioning)
// - **Idempotent**: YES - Uses checksums to track execution
//
// # Standard Configuration
//
// In your global dodot configuration (~/.config/dodot/config.toml):
//
//	[matchers.install-scripts]
//	trigger = "filename"
//	patterns = ["install.sh"]
//	handler = "install_script"
//	priority = 90
//
// Or in a pack-specific .dodot.toml:
//
//	[[matchers]]
//	trigger = "filename"
//	patterns = ["setup.sh", "init.sh"]
//	handler = "install_script"
//
// # File Selection Process
//
// 1. **Pack Discovery**: dodot finds all subdirectories in $DOTFILES_ROOT
// 2. **File Walking**: Recursively walks each pack directory
// 3. **Trigger Matching**: FileNameTrigger matches files (typically "install.sh")
// 4. **Handler Invocation**: Matched files are passed to InstallScriptHandler.Process()
//
// Example file structure:
//
//	~/dotfiles/
//	├── vim/
//	│   └── install.sh      # Installs vim plugins, creates directories
//	├── node/
//	│   └── install.sh      # Installs nvm, sets up global packages
//	└── fonts/
//	    └── install.sh      # Downloads and installs custom fonts
//
// # Execution Strategy
//
// dodot uses a checksum-based sentinel system to ensure scripts run only once:
//
// 1. **First Run**: Script executes, SHA256 checksum is calculated and stored
// 2. **Subsequent Runs**: Current checksum compared with stored value
// 3. **Script Modified**: If checksum differs, script runs again
// 4. **Force Mode**: `--force` flag bypasses checks and re-runs all scripts
//
// Execution process:
//
//  1. Copy script to ~/.local/share/dodot/provision/<pack>/install.sh
//  2. Make script executable (chmod +x)
//  3. Execute with environment variables set
//  4. Store checksum in sentinel file on success
//
// # Storage Locations
//
// - **Copied scripts**: ~/.local/share/dodot/provision/<pack_name>/
// - **Sentinel files**: ~/.local/share/dodot/provision/sentinels/<pack_name>
// - **No cache**: Scripts are always copied fresh from source
// - **Logs**: Script output captured in execution logs
//
// # Environment Variable Tracking
//
// Completed provision scripts are tracked via the `DODOT_PROVISION_SCRIPTS` environment
// variable when dodot-init.sh is sourced:
//
// - **Variable**: `DODOT_PROVISION_SCRIPTS`
// - **Format**: Colon-separated list of pack names with completed provision scripts
// - **Example**: `vim:node:python:tools`
// - **Usage**: `echo $DODOT_PROVISION_SCRIPTS | tr ':' '\n'` to list completed packs
//
// This helps track which packs have had their install scripts run successfully.
//
// # Environment Variables
//
// Scripts execute with these environment variables available:
//
// - **DOTFILES_ROOT**: Path to your dotfiles directory
// - **DODOT_DATA_DIR**: Path to dodot's data directory (~/.local/share/dodot)
// - **DODOT_PACK**: Name of the current pack being provisioned
// - **HOME**: User's home directory
// - **PATH**: System PATH (scripts can modify for their session)
//
// # Effects on User Environment
//
// - **Creates**: Whatever the script creates (directories, files, downloads)
// - **Modifies**: Whatever the script modifies (configs, permissions)
// - **Installs**: External tools, dependencies, packages
// - **Reversible**: Depends on script implementation
// - **Backups**: Script's responsibility to create if needed
//
// # Options
//
// Currently, InstallScriptHandler accepts no configuration options.
// All behavior is controlled by the script itself.
//
// # Script Template
//
// Use `dodot new provision <pack>` to create a script from template:
//
//	#!/usr/bin/env bash
//	set -euo pipefail
//
//	echo "Provisioning <pack>..."
//
//	# Your provisioning commands here
//	# Access $DOTFILES_ROOT, $DODOT_PACK, etc.
//
//	echo "Provisioning complete!"
//
// # Example End-to-End Flow
//
// User runs: `dodot provision`
//
// 1. dodot finds ~/dotfiles/vim/ pack with install.sh
// 2. FileNameTrigger matches install.sh against pattern
// 3. InstallScriptHandler checks sentinel file
// 4. No sentinel or checksum mismatch: proceeds with execution
// 5. Copies install.sh to ~/.local/share/dodot/provision/vim/
// 6. Makes script executable
// 7. Executes with DODOT_PACK=vim, DOTFILES_ROOT=/home/user/dotfiles
// 8. Script installs vim plugins, creates ~/.vim directories
// 9. On success, stores SHA256 checksum in sentinel file
// 10. Future runs skip this script unless modified or --force used
//
// # Error Handling
//
// Common errors and their codes:
//
// - **INST001**: Script file not found
// - **INST002**: Script execution failed (non-zero exit)
// - **INST003**: Script timeout (exceeds 5 minutes)
// - **INST004**: Permission denied (can't make executable)
// - **INST005**: Sentinel write failed
//
// # Best Practices
//
// 1. **Use set -euo pipefail**: Fail fast on errors
// 2. **Check prerequisites**: Verify required tools exist
// 3. **Be idempotent**: Scripts should be safe to run multiple times
// 4. **Log progress**: Use echo statements for visibility
// 5. **Handle errors**: Provide clear error messages
// 6. **Document dependencies**: Comment what the script installs
// 7. **Use DODOT_PACK**: Reference pack name for pack-specific paths
//
// # Common Use Cases
//
// - **Language setup**: Install nvm, rbenv, pyenv
// - **Tool installation**: Download binaries not in package managers
// - **Plugin management**: Install vim/emacs/tmux plugins
// - **Directory creation**: Set up ~/.config structures
// - **Font installation**: Download and install custom fonts
// - **Credentials setup**: Initialize password stores, SSH keys
//
// # Comparison with Other Handlers
//
// - **SymlinkHandler**: For configuration files (deploy phase)
// - **BrewHandler**: For Homebrew packages (also provision phase)
// - **InstallScriptHandler**: For custom setup logic (provision phase)
// - **ProfileHandler**: For shell configuration (link phase)
//
// Use InstallScriptHandler when you need custom logic that package managers
// can't handle, or when you need to perform complex multi-step installations.
package install
