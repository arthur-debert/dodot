package config

// Global configuration instance
var globalConfig *Config

// Initialize sets up the global configuration
func Initialize(cfg *Config) {
	if cfg == nil {
		// Use the new configuration loader instead of Default()
		loadedCfg, err := LoadConfiguration()
		if err != nil {
			// Fallback to hardcoded defaults if loading fails
			cfg = Default()
		} else {
			cfg = loadedCfg
		}
	}
	globalConfig = cfg
}

// Get returns the current configuration
func Get() *Config {
	if globalConfig == nil {
		Initialize(nil)
	}
	return globalConfig
}

// GetPaths returns path configuration
func GetPaths() Paths {
	return Get().Paths
}

// GetSecurity returns security configuration
func GetSecurity() Security {
	return Get().Security
}

// GetPatterns returns pattern configuration
func GetPatterns() Patterns {
	return Get().Patterns
}

// GetFilePermissions returns file permission configuration
func GetFilePermissions() FilePermissions {
	return Get().FilePermissions
}

// GetShellIntegration returns shell integration configuration
func GetShellIntegration() ShellIntegration {
	return Get().ShellIntegration
}

// GetLinkPaths returns link path configuration
func GetLinkPaths() LinkPaths {
	return Get().LinkPaths
}

// GetRules returns rule configuration
func GetRules() []Rule {
	return Get().Rules
}
