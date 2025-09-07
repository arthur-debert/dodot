package config

import (
	"path/filepath"
	"strings"
	"sync"
)

// packConfigRegistry is a thread-safe registry for pack configurations
// This allows handlers to access pack configs without modifying the handler interface
type packConfigRegistry struct {
	configs map[string]PackConfig
	mu      sync.RWMutex
}

var registry = &packConfigRegistry{
	configs: make(map[string]PackConfig),
}

// RegisterPackConfig registers a pack's configuration
func RegisterPackConfig(packName string, config PackConfig) {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	registry.configs[packName] = config
}

// GetPackConfig retrieves a pack's configuration
func GetPackConfig(packName string) (PackConfig, bool) {
	registry.mu.RLock()
	defer registry.mu.RUnlock()
	config, exists := registry.configs[packName]
	return config, exists
}

// ClearPackConfigs clears all registered pack configs (useful for tests)
func ClearPackConfigs() {
	registry.mu.Lock()
	defer registry.mu.Unlock()
	registry.configs = make(map[string]PackConfig)
}

// GetMergedProtectedPaths returns the merged protected paths for a pack
// including both root-level and pack-level protected paths
func GetMergedProtectedPaths(packName string) map[string]bool {
	// Start with root-level protected paths
	merged := GetSecurity().ProtectedPaths

	// Add pack-level protected paths if available
	if packConfig, exists := GetPackConfig(packName); exists {
		return packConfig.GetMergedProtectedPaths(merged)
	}

	return merged
}

// GetForceHomePatterns returns the combined force_home patterns for a pack
// Both pack-level and root-level patterns are included
func GetForceHomePatterns(packName string) []string {
	var patterns []string

	// Start with root-level CoreUnixExceptions
	rootExceptions := GetLinkPaths().CoreUnixExceptions
	for exception := range rootExceptions {
		// Add both the exact match and wildcard pattern for subdirectories
		patterns = append(patterns, exception)
		patterns = append(patterns, exception+"/*")
	}

	// Add pack-level force_home patterns if they exist
	if packConfig, exists := GetPackConfig(packName); exists {
		patterns = append(patterns, packConfig.Symlink.ForceHome...)
	}

	return patterns
}

// IsForceHome checks if a file path matches force_home patterns for a pack
// This combines both root and pack-level configurations with proper precedence
func IsForceHome(packName string, relPath string) bool {
	patterns := GetForceHomePatterns(packName)

	// Get the first segment for root exception matching
	firstSegment := getFirstSegment(relPath)

	// Check if the path matches any pattern
	for _, pattern := range patterns {
		// Try exact match first
		if pattern == relPath {
			return true
		}

		// Check if pattern matches the first segment (for root exceptions)
		if pattern == firstSegment {
			return true
		}

		// Try glob match
		if matched, _ := filepath.Match(pattern, relPath); matched {
			return true
		}

		// Also check against the basename for patterns like "*.conf"
		if matched, _ := filepath.Match(pattern, filepath.Base(relPath)); matched {
			return true
		}
	}

	return false
}

// getFirstSegment returns the first path segment
func getFirstSegment(relPath string) string {
	if relPath == "" {
		return ""
	}

	// Split by separator and return first non-empty segment
	parts := strings.Split(relPath, string(filepath.Separator))
	for _, part := range parts {
		if part != "" {
			return part
		}
	}
	return relPath
}
