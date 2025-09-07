package config

import (
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
