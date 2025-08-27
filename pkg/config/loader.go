package config

import (
	_ "embed"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"

	"github.com/go-viper/mapstructure/v2"
	"github.com/knadh/koanf/parsers/toml"
	"github.com/knadh/koanf/providers/confmap"
	"github.com/knadh/koanf/providers/env"
	"github.com/knadh/koanf/providers/file"
	"github.com/knadh/koanf/v2"
)

//go:embed embedded/defaults.toml
var defaultConfig []byte

//go:embed embedded/user-defaults.toml
var userDefaultConfig []byte

type rawBytesProvider struct{ bytes []byte }

func (r *rawBytesProvider) ReadBytes() ([]byte, error) { return r.bytes, nil }
func (r *rawBytesProvider) Read() (map[string]interface{}, error) {
	return nil, errors.New("not implemented")
}

func LoadConfiguration() (*Config, error) {
	k := koanf.New(".")

	// 1. Manually merge defaults
	baseConfig := getSystemDefaults()
	userDefaults := parseUserDefaults()
	mergeMaps(baseConfig, userDefaults)

	if err := k.Load(confmap.Provider(baseConfig, "."), nil); err != nil {
		return nil, fmt.Errorf("failed to load base config: %w", err)
	}

	// 2. Load user files (will be merged by koanf)
	for _, configPath := range getUserConfigPaths() {
		if _, err := os.Stat(configPath); err == nil {
			// Here we must tell koanf to merge, not just load.
			// Since WithMerge doesn't exist, we can load into a temp koanf and merge manually.
			tempK := koanf.New(".")
			if err := tempK.Load(file.Provider(configPath), toml.Parser()); err != nil {
				return nil, fmt.Errorf("failed to load user config from %s: %w", configPath, err)
			}
			mergeMaps(k.All(), tempK.All())
			break
		}
	}

	// 3. Load env vars
	tempK := koanf.New(".")
	err := tempK.Load(env.Provider("DODOT_", ".", func(s string) string {
		return strings.ReplaceAll(strings.ToLower(strings.TrimPrefix(s, "DODOT_")), "_", ".")
	}), nil)
	if err != nil {
		return nil, fmt.Errorf("failed to load env vars: %w", err)
	}
	mergeMaps(k.All(), tempK.All())

	// 4. Unmarshal
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

func mergeMaps(dest, src map[string]interface{}) {
	for key, srcVal := range src {
		destVal, destOk := dest[key]
		if !destOk {
			dest[key] = srcVal
			continue
		}

		// Merge maps
		if srcMap, srcOk := srcVal.(map[string]interface{}); srcOk {
			if destMap, destOk := destVal.(map[string]interface{}); destOk {
				mergeMaps(destMap, srcMap)
				continue
			}
		}

		// Append slices
		if srcSlice, srcOk := srcVal.([]interface{}); srcOk {
			if destSlice, destOk := destVal.([]interface{}); destOk {
				dest[key] = append(destSlice, srcSlice...)
				continue
			}
		}
		if srcSlice, srcOk := srcVal.([]string); srcOk {
			if destSlice, destOk := destVal.([]string); destOk {
				dest[key] = append(destSlice, srcSlice...)
				continue
			}
		}

		// Otherwise, overwrite
		dest[key] = srcVal
	}
}

func mapToBoolMapHookFunc() mapstructure.DecodeHookFunc {
	return func(f reflect.Type, t reflect.Type, data interface{}) (interface{}, error) {
		if f.Kind() == reflect.Map && t.Kind() == reflect.Map && t.Elem().Kind() == reflect.Bool {
			newMap := make(map[string]bool)
			if m, ok := data.(map[string]interface{}); ok {
				for k, v := range m {
					if b, ok := v.(bool); ok {
						newMap[k] = b
					}
				}
				return newMap, nil
			}
		}
		return data, nil
	}
}

func getUserConfigPaths() []string {
	var paths []string
	if dotfilesRoot := os.Getenv("DOTFILES_ROOT"); dotfilesRoot != "" {
		paths = append(paths, filepath.Join(dotfilesRoot, ".dodot", "config.toml"))
	}
	if xdgConfigHome := os.Getenv("XDG_CONFIG_HOME"); xdgConfigHome != "" {
		paths = append(paths, filepath.Join(xdgConfigHome, "dodot", "config.toml"))
	}
	if homeDir, ok := os.LookupEnv("HOME"); ok {
		paths = append(paths, filepath.Join(homeDir, ".config", "dodot", "config.toml"))
	}
	return paths
}

func getSystemDefaults() map[string]interface{} {
	k := koanf.New(".")
	if err := k.Load(&rawBytesProvider{bytes: defaultConfig}, toml.Parser()); err != nil {
		return map[string]interface{}{}
	}
	return k.All()
}

func parseUserDefaults() map[string]interface{} {
	k := koanf.New(".")
	if err := k.Load(&rawBytesProvider{bytes: userDefaultConfig}, toml.Parser()); err != nil {
		return map[string]interface{}{}
	}
	return transformUserToInternal(k.All())
}

func transformUserToInternal(userConfig map[string]interface{}) map[string]interface{} {
	internal := make(map[string]interface{})
	if pack, ok := userConfig["pack"].(map[string]interface{}); ok {
		if ignore, ok := pack["ignore"]; ok {
			internal = setInMap(internal, []string{"patterns", "pack_ignore"}, ignore)
		}
	}
	if symlink, ok := userConfig["symlink"].(map[string]interface{}); ok {
		if protected, ok := symlink["protected_paths"].([]interface{}); ok {
			internal = setInMap(internal, []string{"security", "protected_paths"}, toBoolMap(protected))
		}
		if forceHome, ok := symlink["force_home"].([]interface{}); ok {
			internal = setInMap(internal, []string{"link_paths", "force_home"}, toBoolMap(forceHome))
		}
	}
	return internal
}

func toBoolMap(s []interface{}) map[string]bool {
	m := make(map[string]bool)
	for _, v := range s {
		if str, ok := v.(string); ok {
			m[str] = true
		}
	}
	return m
}

func setInMap(m map[string]interface{}, keys []string, val interface{}) map[string]interface{} {
	curr := m
	for i, key := range keys {
		if i == len(keys)-1 {
			curr[key] = val
		} else {
			if _, ok := curr[key]; !ok {
				curr[key] = make(map[string]interface{})
			}
			curr = curr[key].(map[string]interface{})
		}
	}
	return m
}

func postProcessConfig(cfg *Config) error {
	if cfg.Patterns.SpecialFiles.PackConfig != "" && cfg.Patterns.SpecialFiles.IgnoreFile != "" {
		cfg.Patterns.CatchallExclude = []string{
			cfg.Patterns.SpecialFiles.PackConfig,
			cfg.Patterns.SpecialFiles.IgnoreFile,
		}
	}
	if len(cfg.Matchers) == 0 {
		cfg.Matchers = defaultMatchers()
	}
	return nil
}
