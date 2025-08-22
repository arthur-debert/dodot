// Package paths provides centralized path handling for dodot.
//
// This package implements the XDG Base Directory specification and provides
// a consistent API for all path operations throughout the dodot codebase.
// It handles:
//
//   - Dotfiles root directory discovery and configuration
//   - XDG directory structure (data, config, cache)
//   - Path normalization and expansion
//   - Pack-specific path generation
//   - State and backup file locations
//
// # Environment Variables
//
// The package respects the following environment variables:
//
//   - DOTFILES_ROOT: Primary location for dotfiles (default: ~/dotfiles)
//   - DODOT_DATA_DIR: Override XDG data directory (default: $XDG_DATA_HOME/dodot)
//   - DODOT_CONFIG_DIR: Override XDG config directory (default: $XDG_CONFIG_HOME/dodot)
//   - DODOT_CACHE_DIR: Override XDG cache directory (default: $XDG_CACHE_HOME/dodot)
//
// # XDG Base Directory Structure
//
// dodot follows the XDG Base Directory specification:
//
//   - Data: $XDG_DATA_HOME/dodot (persistent data, state files, backups)
//   - Config: $XDG_CONFIG_HOME/dodot (user configuration)
//   - Cache: $XDG_CACHE_HOME/dodot (temporary files, caches)
//
// # Usage
//
//	import "github.com/arthur-debert/dodot/pkg/paths"
//
//	// Create a new Paths instance
//	p, err := paths.New("")  // Auto-detect dotfiles root
//	if err != nil {
//	    log.Fatal(err)
//	}
//
//	// Get various paths
//	root := p.DotfilesRoot()                    // /home/user/dotfiles
//	packPath := p.PackPath("vim")               // /home/user/dotfiles/vim
//	stateFile := p.StatePath("vim", "provision")  // $XDG_DATA_HOME/dodot/state/vim/install.json
//
//	// Check if a path is within dotfiles
//	isInside, err := p.IsInDotfiles("/home/user/dotfiles/vim/vimrc")
//	// isInside == true
package paths
