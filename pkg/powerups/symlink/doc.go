// Package symlink provides the SymlinkPowerUp implementation for dodot.
//
// # Overview
//
// The SymlinkPowerUp is one of dodot's core powerups that creates symbolic links from
// dotfiles in your packs to target locations (typically the user's home directory).
// This enables live editing of configuration files - you edit the file in your dotfiles
// repository, and changes immediately affect the deployed location since they're symlinked.
//
// # When It Runs
//
// - **Deploy Mode**: YES - Runs during `dodot deploy` (RunModeMany)
// - **Install Mode**: NO - Does not run during `dodot install`
// - **Idempotent**: YES - Safe to run multiple times
//
// # Standard Configuration
//
// In your global dodot configuration (~/.config/dodot/config.toml):
//
//	[matchers.vim-configs]
//	trigger = "filename"
//	patterns = [".vimrc", ".vim/"]
//	powerup = "symlink"
//	options = { target = "$HOME" }
//	priority = 10
//
// Or in a pack-specific .dodot.toml:
//
//	[[matchers]]
//	trigger = "filename"
//	patterns = ["config/*"]
//	powerup = "symlink"
//	options = { target = "$HOME/.config/myapp" }
//
// # File Selection Process
//
// 1. **Pack Discovery**: dodot finds all subdirectories in $DOTFILES_ROOT
// 2. **File Walking**: Recursively walks each pack directory
// 3. **Trigger Matching**: FileNameTrigger (or others) match files against patterns
// 4. **PowerUp Invocation**: Matched files are passed to SymlinkPowerUp.Process()
//
// Example file structure:
//
//	~/dotfiles/
//	├── vim/
//	│   ├── .vimrc          # Matched by pattern ".vimrc"
//	│   └── .vim/           # Matched by pattern ".vim/"
//	│       └── colors/
//	└── shell/
//	    └── .bashrc         # Matched if pattern includes ".bashrc"
//
// # Two-Link Strategy
//
// dodot uses a sophisticated two-symlink approach for safety and atomicity:
//
// **Step 1 - Intermediate Link** (in dodot's data directory):
//
//	~/.local/share/dodot/deployed/symlink/.vimrc -> ~/dotfiles/vim/.vimrc
//
// **Step 2 - Target Link** (in user's home or specified target):
//
//	~/.vimrc -> ~/.local/share/dodot/deployed/symlink/.vimrc
//
// This strategy provides:
// - Atomic updates (update intermediate without touching user files)
// - Easy identification of dodot-managed symlinks
// - Clean uninstall (all dodot symlinks point to known location)
// - Safe conflict resolution
//
// # Storage Locations
//
// - **Intermediate symlinks**: ~/.local/share/dodot/deployed/symlink/
// - **No sentinels**: Symlinks are idempotent, no need to track state
// - **No cache**: Operations are fast enough to not require caching
//
// # Environment Variable Tracking
//
// The SymlinkPowerUp's deployments are tracked via the `DODOT_SYMLINKS` environment
// variable when dodot-init.sh is sourced:
//
// - **Variable**: `DODOT_SYMLINKS`
// - **Format**: Colon-separated list of relative paths from dotfiles root
// - **Example**: `vim/.vimrc:shell/.bashrc:git/.gitconfig`
// - **Usage**: `echo $DODOT_SYMLINKS | tr ':' '\n'` to list all symlinks
//
// This allows easy debugging and verification of deployed symlinks without
// traversing the filesystem.
//
// # Conflict Resolution
//
// When creating the target symlink (~/.vimrc), dodot handles conflicts:
//
// 1. **Already a dodot symlink**: No action needed (idempotent)
// 2. **Regular file with identical content**: Auto-replace with symlink
// 3. **Different content**: Require --force flag, backup original
// 4. **Different symlink**: Require --force flag
// 5. **Directory**: Always requires --force flag
//
// # Effects on User Environment
//
// - **Creates symlinks in**: User's home directory (or specified target)
// - **Modifies**: Nothing else - purely creates symlinks
// - **Backups**: Original files backed up to ~/.local/share/dodot/backups/ when using --force
// - **Reversible**: Yes - `dodot undeploy` removes all managed symlinks
//
// # Options
//
// The SymlinkPowerUp accepts one option:
//
// - **target** (string): Directory where symlinks should be created
//   - Default: User's home directory (~)
//   - Supports environment variables: $HOME, $XDG_CONFIG_HOME, etc.
//   - Must be an absolute path after expansion
//
// # Example End-to-End Flow
//
// User runs: `dodot deploy`
//
// 1. dodot finds ~/dotfiles/vim/ pack
// 2. FileNameTrigger matches .vimrc against pattern ".vimrc"
// 3. Creates TriggerMatch{PackName: "vim", FilePath: "~/dotfiles/vim/.vimrc"}
// 4. SymlinkPowerUp.Process() receives the match
// 5. Generates Action{Type: "link", Source: "~/dotfiles/vim/.vimrc", Target: "~/.vimrc"}
// 6. DirectExecutor creates intermediate symlink in ~/.local/share/dodot/deployed/symlink/
// 7. DirectExecutor creates/updates target symlink ~/.vimrc
// 8. User can now edit either location with changes reflected in both
//
// # Error Handling
//
// Common errors and their codes:
//
// - **LINK001**: Multiple files targeting same location (e.g., two .vimrc files)
// - **LINK002**: Target directory doesn't exist
// - **LINK003**: Permission denied creating symlink
// - **LINK004**: Conflict requires --force flag
//
// # Best Practices
//
// 1. **Organize by purpose**: Group related configs in same pack
// 2. **Use subdirectories**: Files in vim/.config/nvim/ deploy to ~/.config/nvim/
// 3. **Explicit patterns**: Prefer specific patterns over wildcards
// 4. **Test first**: Use `dodot deploy --dry-run` to preview changes
// 5. **Version control**: Commit your dotfiles for rollback capability
//
// # Comparison with Other PowerUps
//
// - **InstallScriptPowerUp**: Runs once during install, for setup tasks
// - **BrewPowerUp**: Installs packages, runs during install only
// - **ProfilePowerUp**: Modifies shell profiles, runs during deploy
// - **SymlinkPowerUp**: Creates config symlinks, runs during deploy
//
// The SymlinkPowerUp is the workhorse of dodot - most users primarily use it
// to manage their configuration files with live-editing capability.
package symlink
