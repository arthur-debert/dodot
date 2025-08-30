package types

import (
	"fmt"
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

// CreateDirectory creates a directory within the pack
func (p *Pack) CreateDirectory(fs FS, dirname string) error {
	path := p.GetFilePath(dirname)
	return fs.MkdirAll(path, 0755)
}

// CreateFileWithMode creates a file within the pack with specific permissions
func (p *Pack) CreateFileWithMode(fs FS, filename, content string, mode os.FileMode) error {
	path := p.GetFilePath(filename)
	// Ensure parent directory exists
	dir := filepath.Dir(path)
	if err := fs.MkdirAll(dir, 0755); err != nil {
		return err
	}
	return fs.WriteFile(path, []byte(content), mode)
}

// AdoptFile moves an external file into the pack and returns the destination path
func (p *Pack) AdoptFile(fs FS, externalPath, internalPath string, force bool) (string, error) {
	destPath := p.GetFilePath(internalPath)

	// Ensure destination directory exists
	destDir := filepath.Dir(destPath)
	if err := fs.MkdirAll(destDir, 0755); err != nil {
		return "", err
	}

	// Check if destination already exists
	if _, err := fs.Stat(destPath); err == nil {
		if !force {
			return "", fmt.Errorf("destination already exists: %s (use --force to overwrite)", destPath)
		}
		// Remove existing file if force is enabled
		if err := fs.Remove(destPath); err != nil {
			return "", fmt.Errorf("failed to remove existing destination: %w", err)
		}
	}

	// Move the file
	if err := fs.Rename(externalPath, destPath); err != nil {
		return "", fmt.Errorf("failed to move file: %w", err)
	}

	return destPath, nil
}

// CreateIgnoreFile creates a .dodotignore file in the pack
func (p *Pack) CreateIgnoreFile(fs FS, cfg *config.Config) error {
	if cfg == nil {
		cfg = config.Default()
	}
	ignoreFile := cfg.Patterns.SpecialFiles.IgnoreFile
	return p.CreateFile(fs, ignoreFile, "")
}

// HasIgnoreFile checks if the pack has an ignore file
func (p *Pack) HasIgnoreFile(fs FS, cfg *config.Config) (bool, error) {
	if cfg == nil {
		cfg = config.Default()
	}
	ignoreFile := cfg.Patterns.SpecialFiles.IgnoreFile
	return p.FileExists(fs, ignoreFile)
}
