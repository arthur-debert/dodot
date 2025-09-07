package config

import "os"

// This file contains functions to migrate from old complex system to simple koanf system

// GetRootConfig is being replaced - use GetRootConfigNew for now
// Eventually this will be renamed back to GetRootConfig
func GetRootConfig(dotfilesRoot string) (*Config, error) {
	return GetRootConfigNew(dotfilesRoot)
}

// GetPackConfig is being replaced - use GetPackConfigNew for now
// Eventually this will be renamed back to GetPackConfig
func GetPackConfig(rootConfig *Config, packPath string) (*Config, error) {
	return GetPackConfigNew(rootConfig, packPath)
}

// LoadConfiguration is being replaced
func LoadConfiguration() (*Config, error) {
	// Get dotfiles root
	dotfilesRoot := os.Getenv("DOTFILES_ROOT")
	if dotfilesRoot == "" {
		dotfilesRoot = "."
	}

	return GetRootConfigNew(dotfilesRoot)
}
