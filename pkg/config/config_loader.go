package config

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/go-viper/mapstructure/v2"
	"github.com/knadh/koanf/parsers/toml"
	"github.com/knadh/koanf/providers/confmap"
	"github.com/knadh/koanf/providers/file"
	"github.com/knadh/koanf/v2"
)

// GetRootConfig loads and merges app defaults, app config, and root config
// dotfilesRoot: path to dotfiles root directory (required)
// Returns the merged configuration
func GetRootConfig(dotfilesRoot string) (*Config, error) {
	k := koanf.New(".")

	// 1. Load app defaults (embedded defaults.toml)
	appDefaults := getAppDefaults()
	if err := k.Load(confmap.Provider(appDefaults, "."), nil); err != nil {
		return nil, fmt.Errorf("failed to load app defaults: %w", err)
	}

	// 2. Load app config (embedded dodot.toml)
	appConfigMap := getAppConfig()
	mergeMaps(k.All(), appConfigMap)

	// 3. Load root config if it exists
	// Try both .dodot.toml and dodot.toml
	rootConfigPath := ""
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(path); err == nil {
			rootConfigPath = path
			break
		}
	}

	if rootConfigPath != "" {
		tempK := koanf.New(".")
		if err := tempK.Load(file.Provider(rootConfigPath), toml.Parser()); err != nil {
			return nil, fmt.Errorf("failed to load root config from %s: %w", rootConfigPath, err)
		}
		// Transform user format to internal format
		userConfig := transformUserToInternal(tempK.All())
		mergeMaps(k.All(), userConfig)
	}

	// 4. Unmarshal to Config struct
	var cfg Config
	unmarshalConf := koanf.UnmarshalConf{
		Tag: "koanf",
		DecoderConfig: &mapstructure.DecoderConfig{
			Result:           &cfg,
			WeaklyTypedInput: true,
			DecodeHook: mapstructure.ComposeDecodeHookFunc(
				mapstructure.StringToTimeDurationHookFunc(),
				mapstructure.StringToSliceHookFunc(","),
				mapToBoolMapHookFunc(),
			),
		},
	}
	if err := k.UnmarshalWithConf("", &cfg, unmarshalConf); err != nil {
		return nil, fmt.Errorf("failed to unmarshal configuration: %w", err)
	}

	// 5. Post-process
	if err := postProcessConfig(&cfg); err != nil {
		return nil, fmt.Errorf("failed to post-process configuration: %w", err)
	}

	return &cfg, nil
}

// GetPackConfig merges root config with pack-specific config
// rootConfig: the root configuration (from GetRootConfig)
// packPath: path to the pack directory
// Returns the merged configuration for the pack
func GetPackConfig(rootConfig *Config, packPath string) (*Config, error) {
	// Start with a copy of root config
	k := koanf.New(".")

	// Convert root config to map for merging
	rootMap := configToMap(rootConfig)

	if err := k.Load(confmap.Provider(rootMap, "."), nil); err != nil {
		return nil, fmt.Errorf("failed to load root config map: %w", err)
	}

	// Load pack config if it exists
	// Try both .dodot.toml and dodot.toml
	packConfigPath := ""
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(packPath, filename)
		if _, err := os.Stat(path); err == nil {
			packConfigPath = path
			break
		}
	}

	if packConfigPath != "" {
		tempK := koanf.New(".")
		if err := tempK.Load(file.Provider(packConfigPath), toml.Parser()); err != nil {
			return nil, fmt.Errorf("failed to load pack config from %s: %w", packConfigPath, err)
		}
		// Transform user format to internal format
		packConfig := transformUserToInternal(tempK.All())
		mergeMaps(k.All(), packConfig)
	}

	// Unmarshal to Config struct
	var cfg Config
	unmarshalConf := koanf.UnmarshalConf{
		Tag: "koanf",
		DecoderConfig: &mapstructure.DecoderConfig{
			Result:           &cfg,
			WeaklyTypedInput: true,
			DecodeHook: mapstructure.ComposeDecodeHookFunc(
				mapstructure.StringToTimeDurationHookFunc(),
				mapstructure.StringToSliceHookFunc(","),
				mapToBoolMapHookFunc(),
			),
		},
	}
	if err := k.UnmarshalWithConf("", &cfg, unmarshalConf); err != nil {
		return nil, fmt.Errorf("failed to unmarshal pack configuration: %w", err)
	}

	// Post-process
	if err := postProcessConfig(&cfg); err != nil {
		return nil, fmt.Errorf("failed to post-process pack configuration: %w", err)
	}

	return &cfg, nil
}

// getAppDefaults returns the app defaults from embedded defaults.toml
func getAppDefaults() map[string]interface{} {
	k := koanf.New(".")
	if err := k.Load(&rawBytesProvider{bytes: defaultConfig}, toml.Parser()); err != nil {
		return map[string]interface{}{}
	}
	return k.All()
}

// getAppConfig returns the app config from embedded dodot.toml
func getAppConfig() map[string]interface{} {
	if len(appConfig) == 0 {
		return map[string]interface{}{}
	}

	k := koanf.New(".")
	if err := k.Load(&rawBytesProvider{bytes: appConfig}, toml.Parser()); err != nil {
		return map[string]interface{}{}
	}

	return transformUserToInternal(k.All())
}
