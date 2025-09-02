package types

import (
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
)

// Pack represents a directory containing dotfiles and configuration
type Pack struct {
	// Name is the pack name (usually the directory name)
	Name string

	// Path is the absolute path to the pack directory
	Path string

	// Config contains pack-specific configuration from .dodot.toml
	Config config.PackConfig

	// Metadata contains any additional pack information
	Metadata map[string]interface{}
}

// GetFilePath returns the full path to a file within the pack
// This is a fundamental method that computes paths based on the pack's location
func (p *Pack) GetFilePath(filename string) string {
	return filepath.Join(p.Path, filename)
}
