package types

import (
	"os"
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
func (p *Pack) GetFilePath(filename string) string {
	return filepath.Join(p.Path, filename)
}

// FileExists checks if a file exists within the pack
func (p *Pack) FileExists(fs FS, filename string) (bool, error) {
	path := p.GetFilePath(filename)
	_, err := fs.Stat(path)
	if err != nil {
		// Check if it's a "not found" error
		if os.IsNotExist(err) {
			return false, nil
		}
		// For other errors (permission denied, etc.), return the error
		return false, err
	}
	return true, nil
}

// CreateFile creates a file within the pack with the given content
func (p *Pack) CreateFile(fs FS, filename, content string) error {
	path := p.GetFilePath(filename)
	return fs.WriteFile(path, []byte(content), 0644)
}

// ReadFile reads a file from within the pack
func (p *Pack) ReadFile(fs FS, filename string) ([]byte, error) {
	path := p.GetFilePath(filename)
	return fs.ReadFile(path)
}
