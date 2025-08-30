package testutil

import (
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/types"
)

// MockPathResolver provides mock path resolution for testing
type MockPathResolver struct {
	home      string
	xdgConfig string
	xdgData   string
	xdgCache  string
	xdgState  string
	dotfiles  string
}

// NewMockPathResolver creates a new mock path resolver
func NewMockPathResolver(home, xdgConfig, xdgData string) *MockPathResolver {
	return &MockPathResolver{
		home:      home,
		xdgConfig: xdgConfig,
		xdgData:   xdgData,
		xdgCache:  home + "/.cache",
		xdgState:  home + "/.local/state",
		dotfiles:  home + "/dotfiles",
	}
}

// Home returns the home directory path
func (m *MockPathResolver) Home() string {
	return m.home
}

// DotfilesRoot returns the dotfiles root directory
func (m *MockPathResolver) DotfilesRoot() string {
	return m.dotfiles
}

// DataDir returns the XDG data directory
func (m *MockPathResolver) DataDir() string {
	return m.xdgData
}

// ConfigDir returns the XDG config directory
func (m *MockPathResolver) ConfigDir() string {
	return m.xdgConfig
}

// CacheDir returns the XDG cache directory
func (m *MockPathResolver) CacheDir() string {
	return m.xdgCache
}

// StateDir returns the XDG state directory
func (m *MockPathResolver) StateDir() string {
	return m.xdgState
}

// WithDotfilesRoot sets a custom dotfiles root
func (m *MockPathResolver) WithDotfilesRoot(path string) *MockPathResolver {
	m.dotfiles = path
	return m
}

// PackHandlerDir returns the directory for a pack's handler state
func (m *MockPathResolver) PackHandlerDir(packName, handlerName string) string {
	// Mock implementation: put handler state in data dir
	return filepath.Join(m.xdgData, "dodot", "packs", packName, handlerName)
}

// MapPackFileToSystem maps a pack file to its system location
func (m *MockPathResolver) MapPackFileToSystem(pack *types.Pack, relPath string) string {
	// Simple mock implementation
	// For files in root of pack, add dot prefix
	// For files in subdirectories, preserve structure

	parts := strings.Split(relPath, "/")

	if len(parts) == 1 {
		// Top-level file, add dot prefix
		return filepath.Join(m.home, "."+relPath)
	}

	// Subdirectory file
	if parts[0] == ".config" {
		// XDG config file
		return filepath.Join(m.xdgConfig, strings.Join(parts[1:], "/"))
	}

	// Default: preserve structure in home
	return filepath.Join(m.home, relPath)
}
