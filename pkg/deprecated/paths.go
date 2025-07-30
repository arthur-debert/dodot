// Package deprecated contains deprecated functions that are maintained for backward compatibility.
// These functions will be removed in a future version.
// Please use github.com/arthur-debert/dodot/pkg/paths instead.
package deprecated

import (
	"os"
	"path/filepath"
)

// GetDodotDataDir returns the dodot data directory path
// Uses DODOT_DATA_DIR env var if set, otherwise follows XDG Base Directory spec
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetDodotDataDir instead.
func GetDodotDataDir() string {
	if dataDir := os.Getenv("DODOT_DATA_DIR"); dataDir != "" {
		return dataDir
	}

	if xdgDataHome := os.Getenv("XDG_DATA_HOME"); xdgDataHome != "" {
		return filepath.Join(xdgDataHome, "dodot")
	}

	homeDir, _ := os.UserHomeDir()
	return filepath.Join(homeDir, ".local", "share", "dodot")
}

// GetDeployedDir returns the deployed directory path
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetDeployedDir instead.
func GetDeployedDir() string {
	return filepath.Join(GetDodotDataDir(), "deployed")
}

// GetShellProfileDir returns the shell profile deployment directory
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetShellProfileDir instead.
func GetShellProfileDir() string {
	return filepath.Join(GetDeployedDir(), "shell_profile")
}

// GetPathDir returns the PATH deployment directory
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetPathDir instead.
func GetPathDir() string {
	return filepath.Join(GetDeployedDir(), "path")
}

// GetShellSourceDir returns the shell source deployment directory
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetShellSourceDir instead.
func GetShellSourceDir() string {
	return filepath.Join(GetDeployedDir(), "shell_source")
}

// GetSymlinkDir returns the symlink deployment directory
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetSymlinkDir instead.
func GetSymlinkDir() string {
	return filepath.Join(GetDeployedDir(), "symlink")
}

// GetShellDir returns the shell scripts directory
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetShellDir instead.
func GetShellDir() string {
	return filepath.Join(GetDodotDataDir(), "shell")
}

// GetInitScriptPath returns the path to the dodot-init.sh script
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetInitScriptPath instead.
func GetInitScriptPath() string {
	return filepath.Join(GetShellDir(), "dodot-init.sh")
}

// GetInstallDir returns the install scripts sentinel directory
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetInstallDir instead.
func GetInstallDir() string {
	return filepath.Join(GetDodotDataDir(), "install")
}

// GetBrewfileDir returns the brewfile sentinel directory
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetBrewfileDir instead.
func GetBrewfileDir() string {
	return filepath.Join(GetDodotDataDir(), "brewfile")
}
