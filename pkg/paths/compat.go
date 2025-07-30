// Package paths provides centralized path handling for dodot.
// This file contains compatibility functions for migration from old path APIs.
package paths

import (
	"os"
	"path/filepath"
)

// getDefaultPaths returns a default Paths instance for compatibility functions
// Note: This creates a new instance each time to respect environment variable changes
func getDefaultPaths() (*Paths, error) {
	return New("")
}

// GetDodotDataDir returns the dodot data directory path
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetDodotDataDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		// Fallback to basic logic if initialization fails
		if dataDir := os.Getenv(EnvDodotDataDir); dataDir != "" {
			return expandHome(dataDir)
		}
		homeDir := GetHomeDirectoryWithDefault("/tmp")
		return filepath.Join(homeDir, ".local", "share", "dodot")
	}
	return p.DataDir()
}

// GetDeployedDir returns the deployed directory path
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetDeployedDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		return filepath.Join(GetDodotDataDir(), DeployedDir)
	}
	return p.DeployedDir()
}

// GetShellProfileDir returns the shell profile deployment directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetShellProfileDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		return filepath.Join(GetDeployedDir(), "shell_profile")
	}
	return p.ShellProfileDir()
}

// GetPathDir returns the PATH deployment directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetPathDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		return filepath.Join(GetDeployedDir(), "path")
	}
	return p.PathDir()
}

// GetShellSourceDir returns the shell source deployment directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetShellSourceDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		return filepath.Join(GetDeployedDir(), "shell_source")
	}
	return p.ShellSourceDir()
}

// GetSymlinkDir returns the symlink deployment directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetSymlinkDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		return filepath.Join(GetDeployedDir(), "symlink")
	}
	return p.SymlinkDir()
}

// GetShellDir returns the shell scripts directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetShellDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		return filepath.Join(GetDodotDataDir(), ShellDir)
	}
	return p.ShellDir()
}

// GetInitScriptPath returns the path to the dodot-init.sh script
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetInitScriptPath() string {
	p, err := getDefaultPaths()
	if err != nil {
		return filepath.Join(GetShellDir(), InitScriptName)
	}
	return p.InitScriptPath()
}

// GetInstallDir returns the install scripts sentinel directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetInstallDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		return filepath.Join(GetDodotDataDir(), InstallDir)
	}
	return p.InstallDir()
}

// GetBrewfileDir returns the brewfile sentinel directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetBrewfileDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		return filepath.Join(GetDodotDataDir(), BrewfileDir)
	}
	return p.BrewfileDir()
}
