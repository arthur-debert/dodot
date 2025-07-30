package config

import (
	"fmt"
	"os"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/pelletier/go-toml/v2"
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

	// Initialize Files map if nil
	if config.Files == nil {
		config.Files = make(map[string]string)
	}

	logger.Debug().
		Bool("skip", config.Skip).
		Bool("disabled", config.Disabled).
		Bool("ignore", config.Ignore).
		Int("fileRules", len(config.Files)).
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
