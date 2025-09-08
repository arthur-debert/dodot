package config

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/knadh/koanf/parsers/toml"
	"github.com/knadh/koanf/providers/file"
	"github.com/knadh/koanf/v2"
)

// Fields that should append instead of replace when merging configs
var appendFields = map[string]bool{
	"symlink.protected_paths": true,
	"symlink.force_home":      true,
	"pack.ignore":             true,
}

// LoadConfiguration loads the main configuration
func LoadConfiguration() (*Config, error) {
	dotfilesRoot := os.Getenv("DOTFILES_ROOT")
	if dotfilesRoot == "" {
		dotfilesRoot = "."
	}

	k := koanf.NewWithConf(koanf.Conf{
		Delim:       ".",
		StrictMerge: false,
	})

	// Load configs in order: defaults -> app -> root
	providers := []koanf.Provider{
		&rawBytesProvider{bytes: defaultConfig},
		&rawBytesProvider{bytes: appConfig},
	}

	// Add root config if exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(path); err == nil {
			providers = append(providers, file.Provider(path))
			break
		}
	}

	// Load all providers with custom merge
	if err := loadWithArrayAppend(k, providers); err != nil {
		return nil, err
	}

	return unmarshalConfig(k)
}

// GetRootConfig loads root configuration
func GetRootConfig(dotfilesRoot string) (*Config, error) {
	k := koanf.NewWithConf(koanf.Conf{
		Delim:       ".",
		StrictMerge: false,
	})

	// Load configs in order
	providers := []koanf.Provider{
		&rawBytesProvider{bytes: defaultConfig},
		&rawBytesProvider{bytes: appConfig},
	}

	// Add root config if exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(path); err == nil {
			providers = append(providers, file.Provider(path))
			break
		}
	}

	if err := loadWithArrayAppend(k, providers); err != nil {
		return nil, err
	}

	return unmarshalConfig(k)
}

// GetPackConfig loads pack configuration
func GetPackConfig(rootConfig *Config, packPath string) (*Config, error) {
	dotfilesRoot := os.Getenv("DOTFILES_ROOT")
	if dotfilesRoot == "" {
		dotfilesRoot = "."
	}

	k := koanf.NewWithConf(koanf.Conf{
		Delim:       ".",
		StrictMerge: false,
	})

	// Load configs in order: defaults -> app -> root -> pack
	providers := []koanf.Provider{
		&rawBytesProvider{bytes: defaultConfig},
		&rawBytesProvider{bytes: appConfig},
	}

	// Add root config if exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(path); err == nil {
			providers = append(providers, file.Provider(path))
			break
		}
	}

	// Add pack config if exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(packPath, filename)
		if _, err := os.Stat(path); err == nil {
			providers = append(providers, file.Provider(path))
			break
		}
	}

	if err := loadWithArrayAppend(k, providers); err != nil {
		return nil, err
	}

	return unmarshalConfig(k)
}

// loadWithArrayAppend loads providers with custom array append behavior
func loadWithArrayAppend(k *koanf.Koanf, providers []koanf.Provider) error {
	// Track arrays we need to append
	var appendedArrays = make(map[string][]string)

	for _, provider := range providers {
		// Save current values for append fields
		for field := range appendFields {
			if existing := k.Strings(field); len(existing) > 0 {
				appendedArrays[field] = existing
			}
		}

		// Load the provider
		if err := k.Load(provider, toml.Parser()); err != nil {
			return fmt.Errorf("failed to load config: %w", err)
		}

		// Append arrays for specific fields
		for field, saved := range appendedArrays {
			if current := k.Strings(field); len(current) > 0 {
				merged := append(saved, current...)
				_ = k.Set(field, merged)
			}
		}
	}

	return nil
}

// unmarshalConfig unmarshals koanf data into Config struct
func unmarshalConfig(k *koanf.Koanf) (*Config, error) {
	var cfg Config

	// Use koanf's built-in unmarshal
	if err := k.Unmarshal("", &cfg); err != nil {
		return nil, fmt.Errorf("failed to unmarshal config: %w", err)
	}

	// Post-process to set up derived fields
	return postProcessConfig(&cfg)
}

// postProcessConfig handles derived fields and final setup
func postProcessConfig(cfg *Config) (*Config, error) {
	// Convert symlink arrays to internal structures
	cfg.Security.ProtectedPaths = make(map[string]bool)
	for _, path := range cfg.Symlink.ProtectedPaths {
		cfg.Security.ProtectedPaths[path] = true
	}

	cfg.LinkPaths.CoreUnixExceptions = make(map[string]bool)
	for _, path := range cfg.Symlink.ForceHome {
		cfg.LinkPaths.CoreUnixExceptions[path] = true
	}

	// Set up patterns from pack config
	cfg.Patterns.PackIgnore = cfg.Pack.Ignore

	// Ensure special files have defaults
	if cfg.Patterns.SpecialFiles.PackConfig == "" {
		cfg.Patterns.SpecialFiles.PackConfig = ".dodot.toml"
	}
	if cfg.Patterns.SpecialFiles.IgnoreFile == "" {
		cfg.Patterns.SpecialFiles.IgnoreFile = ".dodotignore"
	}

	// Generate rules from mappings
	cfg.Rules = cfg.GenerateRulesFromMapping()
	// Add default rules
	cfg.Rules = append(defaultRules(), cfg.Rules...)

	// Set catchall exclude
	cfg.Patterns.CatchallExclude = []string{
		cfg.Patterns.SpecialFiles.PackConfig,
		cfg.Patterns.SpecialFiles.IgnoreFile,
	}

	return cfg, nil
}
