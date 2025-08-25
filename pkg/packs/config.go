package packs

import (
	"github.com/arthur-debert/dodot/pkg/config"
)

// LoadPackConfig reads and parses a pack's .dodot.toml configuration file
func LoadPackConfig(configPath string) (config.PackConfig, error) {
	return config.LoadPackConfig(configPath)
}
