package testutil

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

// TestEnvironment provides isolated directories for integration testing
type TestEnvironment struct {
	t            *testing.T
	baseDir      string
	dotfilesRoot string
	homeDir      string
	dataDir      string
}

// NewTestEnvironment creates a new test environment with isolated directories
func NewTestEnvironment(t *testing.T, name string) *TestEnvironment {
	t.Helper()

	// Create base temp directory
	baseDir := t.TempDir()

	// Create subdirectories
	dotfilesRoot := filepath.Join(baseDir, "dotfiles")
	homeDir := filepath.Join(baseDir, "home")
	dataDir := filepath.Join(baseDir, "dodot-data")

	// Create all directories
	require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))
	require.NoError(t, os.MkdirAll(homeDir, 0755))
	require.NoError(t, os.MkdirAll(dataDir, 0755))

	// Set environment variables
	t.Setenv("DOTFILES_ROOT", dotfilesRoot)
	t.Setenv("HOME", homeDir)
	t.Setenv("DODOT_DATA_DIR", dataDir)

	return &TestEnvironment{
		t:            t,
		baseDir:      baseDir,
		dotfilesRoot: dotfilesRoot,
		homeDir:      homeDir,
		dataDir:      dataDir,
	}
}

// DotfilesRoot returns the dotfiles root directory
func (te *TestEnvironment) DotfilesRoot() string {
	return te.dotfilesRoot
}

// Home returns the test home directory
func (te *TestEnvironment) Home() string {
	return te.homeDir
}

// DataDir returns the dodot data directory
func (te *TestEnvironment) DataDir() string {
	return te.dataDir
}

// CreatePack creates a new pack directory and returns its path
func (te *TestEnvironment) CreatePack(name string) string {
	packDir := filepath.Join(te.dotfilesRoot, name)
	require.NoError(te.t, os.MkdirAll(packDir, 0755))
	return packDir
}

// Cleanup is a no-op as t.TempDir() handles cleanup automatically
func (te *TestEnvironment) Cleanup() {
	// No-op - t.TempDir() cleans up automatically
}
