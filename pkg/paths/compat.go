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

// GetShellProfileDir returns the shell profile deployment directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetShellProfileDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		// Fallback to basic logic if initialization fails
		dataDir := os.Getenv(EnvDodotDataDir)
		if dataDir == "" {
			homeDir := GetHomeDirectoryWithDefault("/tmp")
			dataDir = filepath.Join(homeDir, ".local", "share", "dodot")
		}
		return filepath.Join(dataDir, DeployedDir, "shell_profile")
	}
	return p.ShellProfileDir()
}

// GetPathDir returns the PATH deployment directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetPathDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		// Fallback to basic logic if initialization fails
		dataDir := os.Getenv(EnvDodotDataDir)
		if dataDir == "" {
			homeDir := GetHomeDirectoryWithDefault("/tmp")
			dataDir = filepath.Join(homeDir, ".local", "share", "dodot")
		}
		return filepath.Join(dataDir, DeployedDir, "path")
	}
	return p.PathDir()
}

// GetSymlinkDir returns the symlink deployment directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetSymlinkDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		// Fallback to basic logic if initialization fails
		dataDir := os.Getenv(EnvDodotDataDir)
		if dataDir == "" {
			homeDir := GetHomeDirectoryWithDefault("/tmp")
			dataDir = filepath.Join(homeDir, ".local", "share", "dodot")
		}
		return filepath.Join(dataDir, DeployedDir, "symlink")
	}
	return p.SymlinkDir()
}

// GetInstallDir returns the install scripts sentinel directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetInstallDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		// Fallback to basic logic if initialization fails
		dataDir := os.Getenv(EnvDodotDataDir)
		if dataDir == "" {
			homeDir := GetHomeDirectoryWithDefault("/tmp")
			dataDir = filepath.Join(homeDir, ".local", "share", "dodot")
		}
		return filepath.Join(dataDir, InstallDir)
	}
	return p.InstallDir()
}

// GetBrewfileDir returns the brewfile sentinel directory
// This is a compatibility wrapper for migration from pkg/types/paths.go
func GetBrewfileDir() string {
	p, err := getDefaultPaths()
	if err != nil {
		// Fallback to basic logic if initialization fails
		dataDir := os.Getenv(EnvDodotDataDir)
		if dataDir == "" {
			homeDir := GetHomeDirectoryWithDefault("/tmp")
			dataDir = filepath.Join(homeDir, ".local", "share", "dodot")
		}
		return filepath.Join(dataDir, BrewfileDir)
	}
	return p.BrewfileDir()
}
