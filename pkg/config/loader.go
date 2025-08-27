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

	// 2. Load root config if it exists
	rootConfigPath := getRootConfigPath()
	if _, err := os.Stat(rootConfigPath); err == nil {
		tempK := koanf.New(".")
		if err := tempK.Load(file.Provider(rootConfigPath), toml.Parser()); err != nil {
			return nil, fmt.Errorf("failed to load root config from %s: %w", rootConfigPath, err)
		}
		// Transform user format to internal format
		userConfig := transformUserToInternal(tempK.All())
		mergeMaps(k.All(), userConfig)
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

// LoadPackConfiguration loads a pack-specific config and merges it with the base config
func LoadPackConfiguration(baseConfig *Config, packPath string) (*Config, error) {
	packConfigPath := filepath.Join(packPath, ".dodot.toml")

	// If no pack config exists, return the base config as-is
	if _, err := os.Stat(packConfigPath); err != nil {
		if os.IsNotExist(err) {
			return baseConfig, nil
		}
		return nil, fmt.Errorf("failed to stat pack config: %w", err)
	}

	// Convert base config to map for merging
	baseMap := configToMap(baseConfig)

	// Load pack config
	tempK := koanf.New(".")
	if err := tempK.Load(file.Provider(packConfigPath), toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load pack config from %s: %w", packConfigPath, err)
	}

	// Transform user format to internal format and merge
	packRaw := tempK.All()
	packConfig := transformUserToInternal(packRaw)

	// Merge pack config into base map
	mergeMaps(baseMap, packConfig)

	// Load the merged data into koanf
	k := koanf.New(".")
	if err := k.Load(confmap.Provider(baseMap, "."), nil); err != nil {
		return nil, fmt.Errorf("failed to load merged config: %w", err)
	}

	// Unmarshal merged config
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

	// Post-process - but don't regenerate matchers here since they were already merged
	if cfg.Patterns.SpecialFiles.PackConfig != "" && cfg.Patterns.SpecialFiles.IgnoreFile != "" {
		cfg.Patterns.CatchallExclude = []string{
			cfg.Patterns.SpecialFiles.PackConfig,
			cfg.Patterns.SpecialFiles.IgnoreFile,
		}
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

		// Append slices - handle various type combinations
		if isSlice(srcVal) && isSlice(destVal) {
			dest[key] = appendSlices(destVal, srcVal)
			continue
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

func getRootConfigPath() string {
	// Look for config in dotfiles root
	if dotfilesRoot := os.Getenv("DOTFILES_ROOT"); dotfilesRoot != "" {
		return filepath.Join(dotfilesRoot, ".dodot.toml")
	}
	// Fall back to current directory
	return ".dodot.toml"
}

func getSystemDefaults() map[string]interface{} {
	k := koanf.New(".")
	if err := k.Load(&rawBytesProvider{bytes: defaultConfig}, toml.Parser()); err != nil {
		return map[string]interface{}{}
	}
	return k.All()
}

func parseUserDefaults() map[string]interface{} {
	if len(userDefaultConfig) == 0 {
		// If embedded is empty for some reason, return empty map
		return map[string]interface{}{}
	}

	k := koanf.New(".")
	if err := k.Load(&rawBytesProvider{bytes: userDefaultConfig}, toml.Parser()); err != nil {
		return map[string]interface{}{}
	}

	return transformUserToInternal(k.All())
}

func transformUserToInternal(userConfig map[string]interface{}) map[string]interface{} {
	// First unflatten the map if it's flattened
	unflattened := unflattenMap(userConfig)

	internal := make(map[string]interface{})
	if pack, ok := unflattened["pack"].(map[string]interface{}); ok {
		if ignore, ok := pack["ignore"]; ok {
			internal = setInMap(internal, []string{"patterns", "pack_ignore"}, ignore)
		}
	}
	if symlink, ok := unflattened["symlink"].(map[string]interface{}); ok {
		if protected, ok := symlink["protected_paths"].([]interface{}); ok {
			internal = setInMap(internal, []string{"security", "protected_paths"}, toBoolMap(protected))
		}
		if forceHome, ok := symlink["force_home"].([]interface{}); ok {
			internal = setInMap(internal, []string{"link_paths", "force_home"}, toBoolMap(forceHome))
		}
	}
	// Pass through file_mapping as-is since it already uses the internal format
	if fileMapping, ok := unflattened["file_mapping"]; ok {
		internal = setInMap(internal, []string{"file_mapping"}, fileMapping)
	}
	return internal
}

func unflattenMap(flat map[string]interface{}) map[string]interface{} {
	result := make(map[string]interface{})

	for key, value := range flat {
		parts := strings.Split(key, ".")
		curr := result

		for i := 0; i < len(parts)-1; i++ {
			if _, ok := curr[parts[i]]; !ok {
				curr[parts[i]] = make(map[string]interface{})
			}
			curr = curr[parts[i]].(map[string]interface{})
		}

		curr[parts[len(parts)-1]] = value
	}

	return result
}

func isSlice(v interface{}) bool {
	switch v.(type) {
	case []interface{}, []string:
		return true
	default:
		return false
	}
}

func appendSlices(dest, src interface{}) interface{} {
	// Convert both to []interface{} for uniform handling
	destSlice := toInterfaceSlice(dest)
	srcSlice := toInterfaceSlice(src)
	return append(destSlice, srcSlice...)
}

func toInterfaceSlice(v interface{}) []interface{} {
	switch s := v.(type) {
	case []interface{}:
		return s
	case []string:
		result := make([]interface{}, len(s))
		for i, v := range s {
			result[i] = v
		}
		return result
	default:
		return []interface{}{}
	}
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

	// Combine default matchers with file_mapping generated matchers
	defaultMatchers := defaultMatchers()
	mappingMatchers := cfg.GenerateMatchersFromMapping()

	// If no matchers are defined, use defaults + mapping
	if len(cfg.Matchers) == 0 {
		cfg.Matchers = append(defaultMatchers, mappingMatchers...)
	} else {
		// Otherwise append mapping matchers to existing
		cfg.Matchers = append(cfg.Matchers, mappingMatchers...)
	}

	return nil
}

// configToMap converts a Config struct to a map for koanf merging
func configToMap(cfg *Config) map[string]interface{} {
	// Create a proper map representation
	m := map[string]interface{}{
		"security": map[string]interface{}{
			"protected_paths": cfg.Security.ProtectedPaths,
		},
	}

	// Build patterns map
	patterns := map[string]interface{}{
		"special_files": map[string]interface{}{
			"pack_config": cfg.Patterns.SpecialFiles.PackConfig,
			"ignore_file": cfg.Patterns.SpecialFiles.IgnoreFile,
		},
	}
	if cfg.Patterns.PackIgnore != nil {
		patterns["pack_ignore"] = cfg.Patterns.PackIgnore
	}
	if cfg.Patterns.CatchallExclude != nil {
		patterns["catchall_exclude"] = cfg.Patterns.CatchallExclude
	}
	m["patterns"] = patterns

	// Build the rest of the config
	m["priorities"] = map[string]interface{}{
		"triggers": cfg.Priorities.Triggers,
		"handlers": cfg.Priorities.Handlers,
		"matchers": cfg.Priorities.Matchers,
	}
	m["file_permissions"] = map[string]interface{}{
		"directory":  cfg.FilePermissions.Directory,
		"file":       cfg.FilePermissions.File,
		"executable": cfg.FilePermissions.Executable,
	}
	m["shell_integration"] = map[string]interface{}{
		"bash_zsh_snippet":             cfg.ShellIntegration.BashZshSnippet,
		"bash_zsh_snippet_with_custom": cfg.ShellIntegration.BashZshSnippetWithCustom,
		"fish_snippet":                 cfg.ShellIntegration.FishSnippet,
	}
	m["link_paths"] = map[string]interface{}{
		"force_home": cfg.LinkPaths.CoreUnixExceptions,
	}

	// Always include file_mapping to ensure proper merging
	fileMapping := make(map[string]interface{})
	fileMapping["path"] = cfg.FileMapping.Path
	fileMapping["install"] = cfg.FileMapping.Install
	fileMapping["homebrew"] = cfg.FileMapping.Homebrew
	if cfg.FileMapping.Shell != nil {
		fileMapping["shell"] = cfg.FileMapping.Shell
	}
	m["file_mapping"] = fileMapping

	// Convert matchers
	if len(cfg.Matchers) > 0 {
		matchers := make([]interface{}, len(cfg.Matchers))
		for i, mc := range cfg.Matchers {
			matchers[i] = map[string]interface{}{
				"name":     mc.Name,
				"priority": mc.Priority,
				"trigger": map[string]interface{}{
					"type": mc.Trigger.Type,
					"data": mc.Trigger.Data,
				},
				"handler": map[string]interface{}{
					"type": mc.Handler.Type,
					"data": mc.Handler.Data,
				},
			}
		}
		m["matchers"] = matchers
	}

	return m
}
