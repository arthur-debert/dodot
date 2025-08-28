package testutil

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
)

// EnvType defines the type of test environment
type EnvType int

const (
	// EnvMemoryOnly uses in-memory filesystem and mocked dependencies (default)
	EnvMemoryOnly EnvType = iota
	// EnvIsolated uses real filesystem in isolated temp directories
	EnvIsolated
	// EnvMocked uses highly controlled mocks for testing edge cases
	EnvMocked
)

// TestEnvironment provides isolated test setup with automatic cleanup
type TestEnvironment struct {
	// Core paths
	DotfilesRoot string
	HomeDir      string
	XDGConfig    string
	XDGData      string
	
	// Core dependencies
	DataStore types.DataStore
	FS        types.FS
	Paths     types.PathResolver
	
	// Environment type
	Type EnvType
	
	// Test context
	t       *testing.T
	tempDir string // Only used for EnvIsolated
	cleanup []func()
}

// EnvOption allows customization of TestEnvironment
type EnvOption func(*TestEnvironment)

// WithHome sets a custom home directory path
func WithHome(path string) EnvOption {
	return func(e *TestEnvironment) {
		e.HomeDir = path
	}
}

// WithDotfilesRoot sets a custom dotfiles root path
func WithDotfilesRoot(path string) EnvOption {
	return func(e *TestEnvironment) {
		e.DotfilesRoot = path
	}
}

// WithXDGConfig sets a custom XDG config directory
func WithXDGConfig(path string) EnvOption {
	return func(e *TestEnvironment) {
		e.XDGConfig = path
	}
}

// WithXDGData sets a custom XDG data directory
func WithXDGData(path string) EnvOption {
	return func(e *TestEnvironment) {
		e.XDGData = path
	}
}

// WithDataStore sets a custom DataStore implementation
func WithDataStore(ds types.DataStore) EnvOption {
	return func(e *TestEnvironment) {
		e.DataStore = ds
	}
}

// WithFS sets a custom filesystem implementation
func WithFS(fs types.FS) EnvOption {
	return func(e *TestEnvironment) {
		e.FS = fs
	}
}

// NewTestEnvironment creates a new test environment with the specified type
func NewTestEnvironment(t *testing.T, envType EnvType, opts ...EnvOption) *TestEnvironment {
	env := &TestEnvironment{
		Type:    envType,
		t:       t,
		cleanup: []func(){},
	}
	
	// Setup based on environment type
	switch envType {
	case EnvMemoryOnly:
		env.setupMemoryEnvironment()
	case EnvIsolated:
		env.setupIsolatedEnvironment()
	case EnvMocked:
		env.setupMockedEnvironment()
	}
	
	// Apply options
	for _, opt := range opts {
		opt(env)
	}
	
	// Set environment variables
	env.setupEnvironmentVariables()
	
	// Register cleanup
	t.Cleanup(env.Cleanup)
	
	return env
}

// setupMemoryEnvironment configures a pure in-memory test environment
func (e *TestEnvironment) setupMemoryEnvironment() {
	// Use virtual paths
	e.DotfilesRoot = "/virtual/dotfiles"
	e.HomeDir = "/virtual/home"
	e.XDGConfig = "/virtual/home/.config"
	e.XDGData = "/virtual/home/.local/share"
	
	// Create memory filesystem
	memFS := NewMemoryFS()
	e.FS = memFS
	
	// Create directories in memory
	memFS.MkdirAll(e.DotfilesRoot, 0755)
	memFS.MkdirAll(e.HomeDir, 0755)
	memFS.MkdirAll(e.XDGConfig, 0755)
	memFS.MkdirAll(filepath.Join(e.XDGData, "dodot"), 0755)
	
	// Create mock datastore
	e.DataStore = NewMockDataStore()
	
	// Create mock path resolver
	e.Paths = NewMockPathResolver(e.HomeDir, e.XDGConfig, e.XDGData)
}

// setupIsolatedEnvironment configures a real filesystem in temp directories
func (e *TestEnvironment) setupIsolatedEnvironment() {
	// Create temp directory
	e.tempDir = e.t.TempDir()
	
	// Setup paths
	e.DotfilesRoot = filepath.Join(e.tempDir, "dotfiles")
	e.HomeDir = filepath.Join(e.tempDir, "home")
	e.XDGConfig = filepath.Join(e.tempDir, "home", ".config")
	e.XDGData = filepath.Join(e.tempDir, "home", ".local", "share")
	
	// Create directories
	os.MkdirAll(e.DotfilesRoot, 0755)
	os.MkdirAll(e.HomeDir, 0755)
	os.MkdirAll(e.XDGConfig, 0755)
	os.MkdirAll(filepath.Join(e.XDGData, "dodot"), 0755)
	
	// Use real filesystem operations (would need to implement RealFS wrapper)
	// For now, we'll use the memory FS as a placeholder
	e.FS = NewMemoryFS()
	
	// Create real datastore (would need to implement)
	// For now, use mock
	e.DataStore = NewMockDataStore()
	
	// Create real path resolver
	e.Paths = NewMockPathResolver(e.HomeDir, e.XDGConfig, e.XDGData)
}

// setupMockedEnvironment configures highly controlled mocks
func (e *TestEnvironment) setupMockedEnvironment() {
	// Similar to memory but with more control points
	e.setupMemoryEnvironment()
	
	// Could add error injection, latency simulation, etc.
}

// setupEnvironmentVariables sets up isolated environment variables
func (e *TestEnvironment) setupEnvironmentVariables() {
	// Save current values for cleanup
	oldHome := os.Getenv("HOME")
	oldDotfiles := os.Getenv("DOTFILES_ROOT")
	oldXDGConfig := os.Getenv("XDG_CONFIG_HOME")
	oldXDGData := os.Getenv("XDG_DATA_HOME")
	oldDodotData := os.Getenv("DODOT_DATA_DIR")
	
	// Set new values
	e.t.Setenv("HOME", e.HomeDir)
	e.t.Setenv("DOTFILES_ROOT", e.DotfilesRoot)
	e.t.Setenv("XDG_CONFIG_HOME", e.XDGConfig)
	e.t.Setenv("XDG_DATA_HOME", e.XDGData)
	e.t.Setenv("DODOT_DATA_DIR", filepath.Join(e.XDGData, "dodot"))
	e.t.Setenv("DODOT_TEST_MODE", "true")
	
	// Register cleanup to restore
	e.cleanup = append(e.cleanup, func() {
		os.Setenv("HOME", oldHome)
		os.Setenv("DOTFILES_ROOT", oldDotfiles)
		os.Setenv("XDG_CONFIG_HOME", oldXDGConfig)
		os.Setenv("XDG_DATA_HOME", oldXDGData)
		os.Setenv("DODOT_DATA_DIR", oldDodotData)
		os.Unsetenv("DODOT_TEST_MODE")
	})
}

// Cleanup performs all cleanup operations
func (e *TestEnvironment) Cleanup() {
	// Run cleanup functions in reverse order
	for i := len(e.cleanup) - 1; i >= 0; i-- {
		e.cleanup[i]()
	}
}

// SetupPack creates a pack with the given configuration
func (e *TestEnvironment) SetupPack(name string, config PackConfig) *TestPack {
	packPath := filepath.Join(e.DotfilesRoot, name)
	
	// Create pack directory
	if err := e.FS.MkdirAll(packPath, 0755); err != nil {
		e.t.Fatalf("failed to create pack directory: %v", err)
	}
	
	// Create files
	for path, content := range config.Files {
		fullPath := filepath.Join(packPath, path)
		dir := filepath.Dir(fullPath)
		
		// Create parent directories
		if err := e.FS.MkdirAll(dir, 0755); err != nil {
			e.t.Fatalf("failed to create directory %s: %v", dir, err)
		}
		
		// Write file
		if err := e.FS.WriteFile(fullPath, []byte(content), 0644); err != nil {
			e.t.Fatalf("failed to write file %s: %v", fullPath, err)
		}
	}
	
	// Create rules file if rules are specified
	if len(config.Rules) > 0 {
		rulesContent := ""
		for _, rule := range config.Rules {
			rulesContent += rule.String() + "\n"
		}
		
		rulesPath := filepath.Join(packPath, ".dodot.toml")
		if err := e.FS.WriteFile(rulesPath, []byte(rulesContent), 0644); err != nil {
			e.t.Fatalf("failed to write rules file: %v", err)
		}
	}
	
	return &TestPack{
		Name: name,
		Path: packPath,
		env:  e,
	}
}

// WithFileTree sets up a complete file tree structure
func (e *TestEnvironment) WithFileTree(tree FileTree) *TestEnvironment {
	for packName, packContents := range tree {
		config := PackConfig{
			Files: make(map[string]string),
		}
		
		// Convert nested structure to flat paths
		flattenFileTree("", packContents, config.Files)
		
		e.SetupPack(packName, config)
	}
	
	return e
}

// flattenFileTree converts nested FileTree to flat map of paths
func flattenFileTree(prefix string, node interface{}, result map[string]string) {
	switch v := node.(type) {
	case string:
		// Leaf node - file content
		result[prefix] = v
	case FileTree:
		// Directory node
		for name, content := range v {
			path := name
			if prefix != "" {
				path = filepath.Join(prefix, name)
			}
			flattenFileTree(path, content, result)
		}
	case map[string]interface{}:
		// Also support regular maps
		for name, content := range v {
			path := name
			if prefix != "" {
				path = filepath.Join(prefix, name)
			}
			flattenFileTree(path, content, result)
		}
	}
}