package types

import (
	"os"
	"path/filepath"
)

// GetDodotDataDir returns the dodot data directory path
// Uses DODOT_DATA_DIR env var if set, otherwise follows XDG Base Directory spec
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
func GetDeployedDir() string {
	return filepath.Join(GetDodotDataDir(), "deployed")
}

// GetShellProfileDir returns the shell profile deployment directory
func GetShellProfileDir() string {
	return filepath.Join(GetDeployedDir(), "shell_profile")
}

// GetPathDir returns the PATH deployment directory
func GetPathDir() string {
	return filepath.Join(GetDeployedDir(), "path")
}

// GetShellSourceDir returns the shell source deployment directory
func GetShellSourceDir() string {
	return filepath.Join(GetDeployedDir(), "shell_source")
}

// GetSymlinkDir returns the symlink deployment directory
func GetSymlinkDir() string {
	return filepath.Join(GetDeployedDir(), "symlink")
}

// GetShellDir returns the shell scripts directory
func GetShellDir() string {
	return filepath.Join(GetDodotDataDir(), "shell")
}

// GetInitScriptPath returns the path to the dodot-init.sh script
func GetInitScriptPath() string {
	return filepath.Join(GetShellDir(), "dodot-init.sh")
}

// GetInstallDir returns the install scripts sentinel directory
func GetInstallDir() string {
	return filepath.Join(GetDodotDataDir(), "install")
}

// GetBrewfileDir returns the brewfile sentinel directory
func GetBrewfileDir() string {
	return filepath.Join(GetDodotDataDir(), "brewfile")
}
