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
	"github.com/knadh/koanf/providers/file"
	"github.com/knadh/koanf/v2"
)

//go:embed embedded/defaults.toml
var defaultConfig []byte

//go:embed embedded/dodot.toml
var appConfig []byte

// GetAppConfigContent returns the content of the app configuration file
func GetAppConfigContent() string {
	return string(appConfig)
}

type rawBytesProvider struct{ bytes []byte }

func (r *rawBytesProvider) ReadBytes() ([]byte, error) { return r.bytes, nil }
func (r *rawBytesProvider) Read() (map[string]interface{}, error) {
	return nil, errors.New("not implemented")
}

func LoadConfiguration() (*Config, error) {
	k := koanf.New(".")

	// 1. Manually merge app defaults and app config
	baseConfig := getSystemDefaults()
	appConfigMap := parseAppConfig()
	mergeMaps(baseConfig, appConfigMap)

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

	// 3. Skip env vars loading (only DOTFILES_ROOT is used, and that's handled separately)

	// 3. Unmarshal
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

	// 4. Post-process
	if err := postProcessConfig(&cfg); err != nil {
		return nil, fmt.Errorf("failed to post-process configuration: %w", err)
	}

	return &cfg, nil
}

// LoadPackConfiguration loads a pack-specific config and merges it with the base config
func LoadPackConfiguration(baseConfig *Config, packPath string) (*Config, error) {
	// Try both .dodot.toml and dodot.toml
	packConfigPath := ""
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(packPath, filename)
		if _, err := os.Stat(path); err == nil {
			packConfigPath = path
			break
		}
	}

	// If neither exists, default to .dodot.toml
	if packConfigPath == "" {
		packConfigPath = filepath.Join(packPath, ".dodot.toml")
	}

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

// Force CI rebuild

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

		// Handle map[string]bool specifically (for force_home, protected_paths)
		if srcBoolMap, srcOk := srcVal.(map[string]bool); srcOk {
			if destBoolMap, destOk := destVal.(map[string]bool); destOk && destBoolMap != nil {
				// Merge bool maps
				for k, v := range srcBoolMap {
					destBoolMap[k] = v
				}
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
	dotfilesRoot := os.Getenv("DOTFILES_ROOT")
	if dotfilesRoot == "" {
		dotfilesRoot = "."
	}

	// Try both .dodot.toml and dodot.toml
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}

	// Default to .dodot.toml if neither exists
	return filepath.Join(dotfilesRoot, ".dodot.toml")
}

func getSystemDefaults() map[string]interface{} {
	k := koanf.New(".")
	if err := k.Load(&rawBytesProvider{bytes: defaultConfig}, toml.Parser()); err != nil {
		return map[string]interface{}{}
	}
	// Unflatten the map to ensure proper nesting for merging
	return unflattenMap(k.All())
}

func parseAppConfig() map[string]interface{} {
	if len(appConfig) == 0 {
		// If embedded is empty for some reason, return empty map
		return map[string]interface{}{}
	}

	k := koanf.New(".")
	if err := k.Load(&rawBytesProvider{bytes: appConfig}, toml.Parser()); err != nil {
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
	// Pass through mappings as-is since it already uses the internal format
	if mappings, ok := unflattened["mappings"]; ok {
		internal = setInMap(internal, []string{"mappings"}, mappings)
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

	// Use a map to track unique values and preserve order
	seen := make(map[string]bool)
	result := make([]interface{}, 0, len(destSlice)+len(srcSlice))

	// Add dest values first (preserving order)
	for _, v := range destSlice {
		if str, ok := v.(string); ok {
			if !seen[str] {
				seen[str] = true
				result = append(result, v)
			}
		} else {
			// Non-string values are kept as-is
			result = append(result, v)
		}
	}

	// Add src values (skipping duplicates)
	for _, v := range srcSlice {
		if str, ok := v.(string); ok {
			if !seen[str] {
				seen[str] = true
				result = append(result, v)
			}
		} else {
			// Non-string values are kept as-is
			result = append(result, v)
		}
	}

	return result
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

	// Combine default rules with mappings generated rules
	defaultRules := defaultRules()
	mappingRules := cfg.GenerateRulesFromMapping()

	// If no rules are defined, use defaults + mapping
	if len(cfg.Rules) == 0 {
		cfg.Rules = append(defaultRules, mappingRules...)
	} else {
		// Otherwise append mapping rules to existing
		cfg.Rules = append(cfg.Rules, mappingRules...)
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

	// Always include mappings to ensure proper merging
	mappings := make(map[string]interface{})
	mappings["path"] = cfg.Mappings.Path
	mappings["install"] = cfg.Mappings.Install
	mappings["homebrew"] = cfg.Mappings.Homebrew
	if cfg.Mappings.Shell != nil {
		mappings["shell"] = cfg.Mappings.Shell
	}
	if cfg.Mappings.Ignore != nil {
		mappings["ignore"] = cfg.Mappings.Ignore
	}
	m["mappings"] = mappings

	// Convert rules
	if len(cfg.Rules) > 0 {
		rules := make([]interface{}, len(cfg.Rules))
		for i, r := range cfg.Rules {
			rules[i] = map[string]interface{}{
				"pattern": r.Pattern,
				"handler": r.Handler,
				"options": r.Options,
			}
		}
		m["rules"] = rules
	}

	return m
}
