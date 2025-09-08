package config

import (
	_ "embed"
	"errors"
)

//go:embed embedded/defaults.toml
var defaultConfig []byte

//go:embed embedded/dodot.toml
var appConfig []byte

// GetAppConfigContent returns the content of the app configuration file
func GetAppConfigContent() string {
	return string(appConfig)
}

// rawBytesProvider implements koanf provider for raw bytes
type rawBytesProvider struct{ bytes []byte }

func (r *rawBytesProvider) ReadBytes() ([]byte, error) { return r.bytes, nil }
func (r *rawBytesProvider) Read() (map[string]interface{}, error) {
	return nil, errors.New("not implemented")
}
