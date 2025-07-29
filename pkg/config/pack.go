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

	// Initialize maps if nil
	if config.PowerUpOptions == nil {
		config.PowerUpOptions = make(map[string]map[string]interface{})
	}

	// Set defaults for matcher enabled state and initialize maps
	for i := range config.Matchers {
		if config.Matchers[i].Enabled == nil {
			enabled := true
			config.Matchers[i].Enabled = &enabled
		}
		if config.Matchers[i].PowerUpOptions == nil {
			config.Matchers[i].PowerUpOptions = make(map[string]interface{})
		}
	}

	logger.Debug().
		Int("matchers", len(config.Matchers)).
		Bool("disabled", config.Disabled).
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