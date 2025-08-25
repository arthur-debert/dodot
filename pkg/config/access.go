package config

// Global configuration instance
var globalConfig *Config

// Initialize sets up the global configuration
func Initialize(cfg *Config) {
	if cfg == nil {
		cfg = Default()
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

// GetPriorities returns priority configuration
func GetPriorities() Priorities {
	return Get().Priorities
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

// GetMatchers returns matcher configuration
func GetMatchers() []MatcherConfig {
	return Get().Matchers
}

// GetLogging returns logging configuration
func GetLogging() LoggingConfig {
	return Get().Logging
}
