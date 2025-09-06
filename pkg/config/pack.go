package config

import (
	"fmt"
	"os"

	toml "github.com/pelletier/go-toml/v2"
)

// PackConfig represents configuration options for a pack from .dodot.toml
type PackConfig struct {
	Mappings Mappings `toml:"mappings"`
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
