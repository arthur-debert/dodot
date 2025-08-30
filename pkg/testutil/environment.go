// pkg/testutil/environment.go
// DEPENDENCIES: None (base test utilities)
// PURPOSE: Orchestrate test environments with proper dependencies

package testutil

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// EnvType defines the type of test environment
type EnvType int

const (
	EnvMemoryOnly EnvType = iota // Pure in-memory, no real filesystem
	EnvIsolated                  // Real filesystem in temp directory
)

// TestEnvironment provides a complete test environment with all dependencies
type TestEnvironment struct {
	// Core paths
	DotfilesRoot string
	HomeDir      string
	XDGData      string

	// Core dependencies
	DataStore types.DataStore
	FS        types.FS
	Paths     types.Pather

	// Environment type
	Type EnvType

	// Test context
	t       *testing.T
	tempDir string // Only used for EnvIsolated
	cleanup []func()
}

// NewTestEnvironment creates a new test environment
func NewTestEnvironment(t *testing.T, envType EnvType) *TestEnvironment {
	t.Helper()

	env := &TestEnvironment{
		t:    t,
		Type: envType,
	}

	switch envType {
	case EnvMemoryOnly:
		env.setupMemoryEnvironment()
	case EnvIsolated:
		env.setupIsolatedEnvironment()
	}

	// Set environment variables
	t.Setenv("DOTFILES_ROOT", env.DotfilesRoot)
	t.Setenv("HOME", env.HomeDir)
	t.Setenv("XDG_DATA_HOME", env.XDGData)

	// Create core dependencies
	if env.Paths == nil {
		pathsInstance, err := paths.New(env.DotfilesRoot)
		if err != nil {
			t.Fatalf("Failed to create paths: %v", err)
		}
		env.Paths = pathsInstance
	}

	if env.DataStore == nil {
		// For memory environment, use mock datastore
		if envType == EnvMemoryOnly {
			env.DataStore = NewMockSimpleDataStore(env.FS)
		}
		// For isolated environment, datastore is created in setupIsolatedEnvironment
	}

	// Ensure cleanup
	t.Cleanup(func() {
		env.Cleanup()
	})

	return env
}

// setupMemoryEnvironment configures a pure in-memory environment
func (env *TestEnvironment) setupMemoryEnvironment() {
	env.DotfilesRoot = "/virtual/dotfiles"
	env.HomeDir = "/virtual/home"
	env.XDGData = "/virtual/home/.local/share"

	// Create memory filesystem
	env.FS = NewMemoryFS()

	// Create base directories
	_ = env.FS.MkdirAll(env.DotfilesRoot, 0755)
	_ = env.FS.MkdirAll(env.HomeDir, 0755)
	_ = env.FS.MkdirAll(env.XDGData, 0755)
}

// setupIsolatedEnvironment configures a real filesystem in temp directory
func (env *TestEnvironment) setupIsolatedEnvironment() {
	// Create temp directory
	tempDir := env.t.TempDir()
	env.tempDir = tempDir

	// Set up paths
	env.DotfilesRoot = filepath.Join(tempDir, "dotfiles")
	env.HomeDir = filepath.Join(tempDir, "home")
	env.XDGData = filepath.Join(tempDir, "home", ".local", "share")

	// Create real filesystem
	env.FS = filesystem.NewOS()

	// Create base directories
	_ = env.FS.MkdirAll(env.DotfilesRoot, 0755)
	_ = env.FS.MkdirAll(env.HomeDir, 0755)
	_ = env.FS.MkdirAll(env.XDGData, 0755)

	// For isolated environment, create real paths and datastore
	pathsInstance, err := paths.New(env.DotfilesRoot)
	if err != nil {
		env.t.Fatalf("Failed to create paths: %v", err)
	}
	env.Paths = pathsInstance

	// Create real datastore
	env.DataStore = &realDataStore{
		fs:      env.FS,
		dataDir: filepath.Join(env.XDGData, "dodot", "data"),
	}
}

// Cleanup performs environment cleanup
func (env *TestEnvironment) Cleanup() {
	// Run any registered cleanup functions
	for _, fn := range env.cleanup {
		fn()
	}
}

// SetupPack creates a test pack with the given configuration
func (env *TestEnvironment) SetupPack(name string, config PackConfig) types.Pack {
	env.t.Helper()

	packPath := filepath.Join(env.DotfilesRoot, name)
	if err := env.FS.MkdirAll(packPath, 0755); err != nil {
		env.t.Fatalf("Failed to create pack directory: %v", err)
	}

	// Create files
	for filePath, content := range config.Files {
		fullPath := filepath.Join(packPath, filePath)

		// Create parent directory if needed
		if dir := filepath.Dir(fullPath); dir != "." {
			if err := env.FS.MkdirAll(dir, 0755); err != nil {
				env.t.Fatalf("Failed to create directory %s: %v", dir, err)
			}
		}

		// Write file
		if err := env.FS.WriteFile(fullPath, []byte(content), 0644); err != nil {
			env.t.Fatalf("Failed to write file %s: %v", filePath, err)
		}
	}

	// Create rules file if rules are provided
	if len(config.Rules) > 0 {
		rulesContent := generateRulesFile(config.Rules)
		rulesPath := filepath.Join(packPath, ".dodot.toml")
		if err := env.FS.WriteFile(rulesPath, []byte(rulesContent), 0644); err != nil {
			env.t.Fatalf("Failed to write rules file: %v", err)
		}
	}

	return types.Pack{
		Name: name,
		Path: packPath,
	}
}

// WithFileTree creates a complete file tree structure
func (env *TestEnvironment) WithFileTree(tree FileTree) {
	env.t.Helper()
	createFileTree(env.t, env.FS, env.DotfilesRoot, tree)
}

// PackConfig defines configuration for a test pack
type PackConfig struct {
	Files map[string]string // Path -> Content
	Rules []Rule            // Rules configuration
}

// Rule represents a test rule configuration
type Rule struct {
	Type    string // "filename", "directory", etc.
	Pattern string
	Handler string
	Options map[string]interface{}
}

// FileTree represents a directory structure for testing
type FileTree map[string]interface{}

// createFileTree recursively creates a file tree
func createFileTree(t *testing.T, fs types.FS, basePath string, tree FileTree) {
	t.Helper()

	for name, content := range tree {
		fullPath := filepath.Join(basePath, name)

		switch v := content.(type) {
		case string:
			// It's a file
			if err := fs.WriteFile(fullPath, []byte(v), 0644); err != nil {
				t.Fatalf("Failed to write file %s: %v", fullPath, err)
			}
		case FileTree:
			// It's a directory
			if err := fs.MkdirAll(fullPath, 0755); err != nil {
				t.Fatalf("Failed to create directory %s: %v", fullPath, err)
			}
			createFileTree(t, fs, fullPath, v)
		default:
			t.Fatalf("Invalid file tree content type for %s: %T", name, content)
		}
	}
}

// generateRulesFile creates a TOML rules file content
func generateRulesFile(rules []Rule) string {
	content := ""
	for _, rule := range rules {
		content += "[[rules]]\n"
		content += fmt.Sprintf("type = %q\n", rule.Type)
		content += fmt.Sprintf("pattern = %q\n", rule.Pattern)
		content += fmt.Sprintf("handler = %q\n", rule.Handler)

		if len(rule.Options) > 0 {
			content += "[rules.options]\n"
			for k, v := range rule.Options {
				content += fmt.Sprintf("%s = %q\n", k, v)
			}
		}
		content += "\n"
	}
	return content
}

// Pre-built pack configurations for common test scenarios

// VimPack returns a pre-configured vim pack
func VimPack() PackConfig {
	return PackConfig{
		Files: map[string]string{
			"vimrc":              "\" Vim configuration\nset number\nset expandtab",
			"gvimrc":             "\" GVim configuration\nset guifont=Monaco:h12",
			"colors/monokai.vim": "\" Monokai color scheme",
		},
		Rules: []Rule{
			{Type: "filename", Pattern: "vimrc", Handler: "symlink"},
			{Type: "filename", Pattern: "gvimrc", Handler: "symlink"},
		},
	}
}

// GitPack returns a pre-configured git pack
func GitPack() PackConfig {
	return PackConfig{
		Files: map[string]string{
			"gitconfig": "[user]\n  name = Test User\n  email = test@example.com",
			"gitignore": "*.tmp\n*.log\n.DS_Store",
		},
		Rules: []Rule{
			{Type: "filename", Pattern: "git*", Handler: "symlink"},
		},
	}
}

// ShellPack returns a pre-configured shell pack
func ShellPack() PackConfig {
	return PackConfig{
		Files: map[string]string{
			"aliases.sh":   "alias ll='ls -la'\nalias gs='git status'",
			"functions.sh": "function mkcd() { mkdir -p \"$1\" && cd \"$1\"; }",
			"profile.sh":   "export EDITOR=vim\nexport LANG=en_US.UTF-8",
		},
		Rules: []Rule{
			{Type: "filename", Pattern: "*.sh", Handler: "shell"},
		},
	}
}

// ToolsPack returns a pre-configured tools pack
func ToolsPack() PackConfig {
	return PackConfig{
		Files: map[string]string{
			"bin/my-script":    "#!/bin/bash\necho 'Hello from my-script'",
			"bin/another-tool": "#!/bin/bash\necho 'Another tool'",
			"install.sh":       "#!/bin/bash\necho 'Installing tools...'",
			"Brewfile":         "brew 'git'\nbrew 'vim'\nbrew 'tmux'",
		},
		Rules: []Rule{
			{Type: "directory", Pattern: "bin/", Handler: "path"},
			{Type: "filename", Pattern: "install.sh", Handler: "install"},
			{Type: "filename", Pattern: "Brewfile", Handler: "homebrew"},
		},
	}
}

// MockSimpleDataStore provides a simple mock DataStore for testing
type MockSimpleDataStore struct {
	dataLinks map[string]string // pack:handler:source -> datastorePath
	userLinks map[string]string // target -> source
	sentinels map[string]bool   // pack:handler:sentinel -> exists
	commands  map[string]string // pack:handler:sentinel -> command
	fs        types.FS
}

// NewMockSimpleDataStore creates a new mock datastore
func NewMockSimpleDataStore(fs types.FS) *MockSimpleDataStore {
	return &MockSimpleDataStore{
		dataLinks: make(map[string]string),
		userLinks: make(map[string]string),
		sentinels: make(map[string]bool),
		commands:  make(map[string]string),
		fs:        fs,
	}
}

func (m *MockSimpleDataStore) CreateDataLink(pack, handlerName, sourceFile string) (string, error) {
	key := fmt.Sprintf("%s:%s:%s", pack, handlerName, sourceFile)
	datastorePath := fmt.Sprintf("/datastore/%s/%s/%s", pack, handlerName, filepath.Base(sourceFile))
	m.dataLinks[key] = datastorePath
	return datastorePath, nil
}

func (m *MockSimpleDataStore) CreateUserLink(datastorePath, userPath string) error {
	m.userLinks[userPath] = datastorePath
	return nil
}

func (m *MockSimpleDataStore) RunAndRecord(pack, handlerName, command, sentinel string) error {
	key := fmt.Sprintf("%s:%s:%s", pack, handlerName, sentinel)
	m.sentinels[key] = true
	m.commands[key] = command
	return nil
}

func (m *MockSimpleDataStore) HasSentinel(pack, handlerName, sentinel string) (bool, error) {
	key := fmt.Sprintf("%s:%s:%s", pack, handlerName, sentinel)
	return m.sentinels[key], nil
}

func (m *MockSimpleDataStore) RemoveState(pack, handlerName string) error {
	// Remove all entries for this pack/handler combination
	prefix := fmt.Sprintf("%s:%s:", pack, handlerName)

	// Remove data links
	for key := range m.dataLinks {
		if len(key) >= len(prefix) && key[:len(prefix)] == prefix {
			delete(m.dataLinks, key)
		}
	}

	// Remove sentinels
	for key := range m.sentinels {
		if len(key) >= len(prefix) && key[:len(prefix)] == prefix {
			delete(m.sentinels, key)
		}
	}

	return nil
}

// Helper methods for testing
func (m *MockSimpleDataStore) GetDataLinks() map[string]string { return m.dataLinks }
func (m *MockSimpleDataStore) GetUserLinks() map[string]string { return m.userLinks }
func (m *MockSimpleDataStore) GetSentinels() map[string]bool   { return m.sentinels }
func (m *MockSimpleDataStore) GetCommands() map[string]string  { return m.commands }

// realDataStore implements a minimal DataStore using real filesystem
type realDataStore struct {
	fs      types.FS
	dataDir string
}

func (d *realDataStore) CreateDataLink(pack, handlerName, sourceFile string) (string, error) {
	baseName := filepath.Base(sourceFile)
	intermediateLinkDir := filepath.Join(d.dataDir, pack, handlerName)
	intermediateLinkPath := filepath.Join(intermediateLinkDir, baseName)

	if err := d.fs.MkdirAll(intermediateLinkDir, 0755); err != nil {
		return "", err
	}

	// If the link already exists and points to the correct source, do nothing
	if currentTarget, err := d.fs.Readlink(intermediateLinkPath); err == nil && currentTarget == sourceFile {
		return intermediateLinkPath, nil
	}

	// If it exists but is wrong, remove it first
	if _, err := d.fs.Lstat(intermediateLinkPath); err == nil {
		if err := d.fs.Remove(intermediateLinkPath); err != nil {
			return "", err
		}
	}

	if err := d.fs.Symlink(sourceFile, intermediateLinkPath); err != nil {
		return "", err
	}

	return intermediateLinkPath, nil
}

func (d *realDataStore) CreateUserLink(datastorePath, userPath string) error {
	// Create parent directory if needed
	if err := d.fs.MkdirAll(filepath.Dir(userPath), 0755); err != nil {
		return err
	}

	// If link already exists, remove it
	if _, err := d.fs.Lstat(userPath); err == nil {
		if err := d.fs.Remove(userPath); err != nil {
			return err
		}
	}

	return d.fs.Symlink(datastorePath, userPath)
}

func (d *realDataStore) RunAndRecord(pack, handlerName, command, sentinel string) error {
	// For test environment, just record the sentinel
	sentinelDir := filepath.Join(d.dataDir, pack, handlerName)
	sentinelPath := filepath.Join(sentinelDir, sentinel)

	if err := d.fs.MkdirAll(sentinelDir, 0755); err != nil {
		return err
	}

	content := fmt.Sprintf("completed|%s", time.Now().Format(time.RFC3339))
	return d.fs.WriteFile(sentinelPath, []byte(content), 0644)
}

func (d *realDataStore) HasSentinel(pack, handlerName, sentinel string) (bool, error) {
	sentinelPath := filepath.Join(d.dataDir, pack, handlerName, sentinel)
	_, err := d.fs.Stat(sentinelPath)
	if err == nil {
		return true, nil
	}
	if os.IsNotExist(err) {
		return false, nil
	}
	return false, err
}

func (d *realDataStore) RemoveState(pack, handlerName string) error {
	stateDir := filepath.Join(d.dataDir, pack, handlerName)
	return d.fs.RemoveAll(stateDir)
}
