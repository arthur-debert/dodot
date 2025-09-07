package config

import (
	"fmt"
	"os"
	"path/filepath"

	toml "github.com/pelletier/go-toml/v2"
)

// PackConfig represents configuration options for a pack from .dodot.toml
type PackConfig struct {
	Mappings Mappings `toml:"mappings"`
	Symlink  Symlink  `toml:"symlink"`
}

// Symlink holds symlink-specific configuration
type Symlink struct {
	// ForceHome lists files that should always deploy to $HOME
	ForceHome []string `toml:"force_home"`
	// ProtectedPaths lists additional paths that should not be symlinked
	ProtectedPaths []string `toml:"protected_paths"`
}

// LoadPackConfig reads and parses a pack's .dodot.toml configuration file
func LoadPackConfig(configPath string) (PackConfig, error) {
	data, err := os.ReadFile(configPath)
	if err != nil {
		return PackConfig{}, fmt.Errorf("failed to read config file: %w", err)
	}

	var config PackConfig
	if err := toml.Unmarshal(data, &config); err != nil {
		return PackConfig{}, fmt.Errorf("failed to parse TOML: %w", err)
	}

	return config, nil
}

// FileExists is a helper to check if a file exists
func FileExists(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir()
}

// IsForceHome checks if a file path matches any force_home pattern in the pack config
func (pc *PackConfig) IsForceHome(relPath string) bool {
	for _, pattern := range pc.Symlink.ForceHome {
		// Support both exact matches and glob patterns
		if pattern == relPath {
			return true
		}

		// Try glob matching
		matched, err := filepath.Match(pattern, relPath)
		if err == nil && matched {
			return true
		}

		// Also check the base name for patterns like "*.conf"
		if matched, err := filepath.Match(pattern, filepath.Base(relPath)); err == nil && matched {
			return true
		}
	}
	return false
}

// GetMergedProtectedPaths returns a merged map of protected paths from both root and pack configs
func (pc *PackConfig) GetMergedProtectedPaths(rootProtected map[string]bool) map[string]bool {
	// Start with a copy of root protected paths
	merged := make(map[string]bool)
	for path := range rootProtected {
		merged[path] = true
	}

	// Add pack-level protected paths
	for _, path := range pc.Symlink.ProtectedPaths {
		merged[path] = true
	}

	return merged
}
