// Package path provides the PathHandler implementation for dodot.
//
// # Overview
//
// The PathHandler automatically adds directories from your dotfile packs to the system PATH.
// This is perfect for managing personal scripts, custom binaries, and development tools
// organized in `bin/` or `scripts/` directories within your dotfiles.
//
// # When It Runs
//
// - **Deploy Mode**: YES - Runs during `dodot deploy` (RunModeMany)
// - **Install Mode**: NO - Does not run during `dodot install`
// - **Idempotent**: YES - Safe to run multiple times, prevents duplicates
//
// # Standard Configuration
//
// In your global dodot configuration (~/.config/dodot/config.toml):
//
//	[matchers.path-directories]
//	trigger = "directory"
//	patterns = ["bin", "scripts", ".local/bin"]
//	handler = "path"
//	priority = 50
//
// Or in a pack-specific .dodot.toml:
//
//	[[matchers]]
//	trigger = "directory"
//	patterns = ["tools", "commands"]
//	handler = "path"
//
// # File Selection Process
//
// 1. **Pack Discovery**: dodot finds all subdirectories in $DOTFILES_ROOT
// 2. **Directory Walking**: Recursively walks each pack directory
// 3. **Trigger Matching**: DirectoryTrigger matches directories by name
// 4. **Handler Invocation**: Matched directories are passed to PathHandler.Process()
//
// Example file structure:
//
//	~/dotfiles/
//	├── tools/
//	│   └── bin/           # Added to PATH as "tools-bin"
//	│       ├── mytool
//	│       └── helper.sh
//	├── node/
//	│   └── scripts/       # Added to PATH as "node-scripts"
//	│       └── npm-utils
//	└── python/
//	    └── .local/
//	        └── bin/       # Added to PATH as "python-.local-bin"
//	            └── pylint-wrapper
//
// # Execution Strategy
//
// dodot uses a two-stage approach for PATH management:
//
// **Stage 1 - Create Symlinks**:
// - Creates symlinks in ~/.local/share/dodot/deployed/path/
// - Symlink names: `<pack>-<dirname>` (e.g., `tools-bin`)
// - Points to actual directories in your dotfiles
//
// **Stage 2 - Update Shell Init**:
// - Appends PATH exports to ~/.local/share/dodot/shell/init.sh
// - Each export adds the symlinked directory to PATH
// - Checks for existing entries to ensure idempotency
//
// This approach provides:
// - Clean PATH entries (no long dotfiles paths)
// - Easy identification of dodot-managed paths
// - Safe updates without shell restarts
// - Simple removal by deleting symlinks
//
// # Storage Locations
//
// - **Path symlinks**: ~/.local/share/dodot/deployed/path/<pack>-<dirname>
// - **Shell exports**: ~/.local/share/dodot/shell/init.sh
// - **No sentinels**: PATH additions are idempotent by design
// - **No cache**: Fast enough to not require caching
//
// # Environment Variable Tracking
//
// PATH additions are tracked via the `DODOT_PATH_DIRS` environment variable
// when dodot-init.sh is sourced:
//
// - **Variable**: `DODOT_PATH_DIRS`
// - **Format**: Colon-separated list of relative paths from dotfiles root
// - **Example**: `tools/bin:scripts/bin:python/.local/bin`
// - **Usage**: `echo $DODOT_PATH_DIRS | tr ':' '\n'` to list all PATH additions
//
// This helps debug PATH issues and see which directories dodot has added to your PATH.
//
// # Shell Integration
//
// After deployment, your shell's init.sh will contain:
//
//	# Add bin to PATH from tools
//	export PATH="$HOME/.local/share/dodot/deployed/path/tools-bin:$PATH"
//
//	# Add scripts to PATH from node
//	export PATH="$HOME/.local/share/dodot/deployed/path/node-scripts:$PATH"
//
// These are sourced on shell startup via dodot-init.sh.
//
// # Effects on User Environment
//
// - **Modifies PATH**: Prepends directories to system PATH
// - **Creates symlinks**: In dodot's data directory
// - **Updates shell init**: Adds export statements
// - **Reversible**: Remove symlinks and init.sh entries to undo
// - **Order matters**: Later packs' paths take precedence (prepended)
//
// # Options
//
// The PathHandler accepts these options:
//
// - **target** (string): Currently reserved for future use
//
// # Example End-to-End Flow
//
// User runs: `dodot deploy`
//
//  1. dodot finds ~/dotfiles/tools/bin/ directory
//  2. DirectoryTrigger matches "bin" pattern
//  3. PathHandler creates ActionTypePathAdd for the directory
//  4. DirectExecutor creates symlink:
//     ~/.local/share/dodot/deployed/path/tools-bin -> ~/dotfiles/tools/bin
//  5. Checks ~/.local/share/dodot/shell/init.sh for existing entry
//  6. Appends: `export PATH="$HOME/.local/share/dodot/deployed/path/tools-bin:$PATH"`
//  7. On next shell startup, tools in bin/ are available in PATH
//  8. User can add new scripts to bin/ without re-deploying
//
// # Error Handling
//
// Common errors and their codes:
//
// - **PATH001**: Directory doesn't exist
// - **PATH002**: Permission denied creating symlink
// - **PATH003**: Symlink conflict (different target)
// - **PATH004**: Can't write to init.sh
// - **PATH005**: Invalid directory name
//
// # Best Practices
//
// 1. **Use standard names**: Prefer `bin/`, `scripts/`, `.local/bin/`
// 2. **Make scripts executable**: Use `chmod +x` on your scripts
// 3. **Add shebangs**: Start scripts with `#!/usr/bin/env bash`
// 4. **Avoid name conflicts**: Prefix tools to avoid PATH collisions
// 5. **Document tools**: Add README in bin/ directories
// 6. **Test scripts**: Ensure they work before deploying
// 7. **Use subdirectories**: Organize by language or purpose
//
// # PATH Ordering
//
// Directories are prepended to PATH in the order they're processed:
// - Later packs override earlier ones
// - Within a pack, alphabetical order applies
// - User can control via pack priorities in config
//
// To see the order:
//
//	echo $PATH | tr ':' '\n' | grep dodot
//
// # Security Considerations
//
// - **Only from packs**: Only directories within dotfile packs are added
// - **Symlink verification**: Symlinks are verified before creation
// - **No arbitrary paths**: Can't add paths outside dotfiles structure
// - **User-owned**: All paths must be readable by the user
//
// # Comparison with Other Handlers
//
// - **ShellProfileHandler**: For general shell configuration
// - **PathHandler**: Specifically for PATH modifications
// - **SymlinkHandler**: For individual file links
// - **InstallScriptHandler**: For one-time setup
//
// Use PathHandler when you have directories of executables or scripts that
// should be available system-wide. It's the cleanest way to manage personal
// tools and scripts across multiple machines.
package path
