package config

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"

	"github.com/knadh/koanf/parsers/toml"
	"github.com/knadh/koanf/providers/file"
	"github.com/knadh/koanf/v2"
)

// NewSimpleConfig loads configuration using koanf without any transformations
func NewSimpleConfig() (*koanf.Koanf, error) {
	k := koanf.New(".")

	// 1. Load system defaults
	if err := k.Load(&rawBytesProvider{bytes: defaultConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load defaults: %w", err)
	}

	// 2. Load app config (embedded dodot.toml)
	if err := k.Load(&rawBytesProvider{bytes: appConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load app config: %w", err)
	}

	// 3. Load root config if it exists
	dotfilesRoot := os.Getenv("DOTFILES_ROOT")
	if dotfilesRoot == "" {
		dotfilesRoot = "."
	}

	// Try both .dodot.toml and dodot.toml
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(path); err == nil {
			if err := k.Load(file.Provider(path), toml.Parser()); err != nil {
				return nil, fmt.Errorf("failed to load root config from %s: %w", path, err)
			}
			break
		}
	}

	return k, nil
}

// GetSimpleRootConfig loads root configuration without transformations
func GetSimpleRootConfig(dotfilesRoot string) (*koanf.Koanf, error) {
	k := koanf.New(".")

	// 1. Load defaults
	if err := k.Load(&rawBytesProvider{bytes: defaultConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load defaults: %w", err)
	}

	// 2. Load app config
	if err := k.Load(&rawBytesProvider{bytes: appConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load app config: %w", err)
	}

	// 3. Load root config if it exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(path); err == nil {
			if err := k.Load(file.Provider(path), toml.Parser()); err != nil {
				return nil, fmt.Errorf("failed to load root config from %s: %w", path, err)
			}
			break
		}
	}

	return k, nil
}

// GetSimplePackConfig loads pack configuration merged with base
// dotfilesRoot: path to root dotfiles directory
// packPath: path to pack directory
func GetSimplePackConfig(dotfilesRoot, packPath string) (*koanf.Koanf, error) {
	// To get proper override behavior, we need to reload all configs in order
	k := koanf.New(".")

	// 1. Load defaults
	if err := k.Load(&rawBytesProvider{bytes: defaultConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load defaults: %w", err)
	}

	// 2. Load app config
	if err := k.Load(&rawBytesProvider{bytes: appConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load app config: %w", err)
	}

	// 3. Load root config if it exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		rootPath := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(rootPath); err == nil {
			if err := k.Load(file.Provider(rootPath), toml.Parser()); err != nil {
				return nil, fmt.Errorf("failed to load root config from %s: %w", rootPath, err)
			}
			break
		}
	}

	// 4. Load pack config if it exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(packPath, filename)
		if _, err := os.Stat(path); err == nil {
			if err := k.Load(file.Provider(path), toml.Parser()); err != nil {
				return nil, fmt.Errorf("failed to load pack config from %s: %w", path, err)
			}
			break
		}
	}

	return k, nil
}
