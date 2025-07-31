package config

import (
	"fmt"
	"os"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	toml "github.com/pelletier/go-toml/v2"
)

var log = logging.GetLogger("config")

// LoadPackConfig reads and parses a pack's .dodot.toml configuration file
func LoadPackConfig(configPath string) (types.PackConfig, error) {
	logger := log.With().Str("configPath", configPath).Logger()

	data, err := os.ReadFile(configPath)
	if err != nil {
		return types.PackConfig{}, fmt.Errorf("failed to read config file: %w", err)
	}

	var config types.PackConfig
	if err := toml.Unmarshal(data, &config); err != nil {
		return types.PackConfig{}, fmt.Errorf("failed to parse TOML: %w", err)
	}

	logger.Debug().
		Int("ignore_rules", len(config.Ignore)).
		Int("override_rules", len(config.Override)).
		Msg("Pack config loaded")

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
