package triggers_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestProcessPackTriggers_CatchallBehavior(t *testing.T) {
	// Create test directory structure
	testDir := testutil.TempDir(t, "catchall-test")
	packDir := filepath.Join(testDir, "test-pack")
	testutil.CreateDir(t, testDir, "test-pack")

	// Create various test files
	testFiles := map[string]string{
		".vimrc":       "vim config",     // Should match vim-config
		".bashrc":      "bash config",    // Should match bash-config
		"Brewfile":     "brew 'git'",     // Should match brewfile
		"install.sh":   "#!/bin/bash",    // Should match install-script
		"custom.conf":  "custom config",  // Should match catchall
		"random.txt":   "random content", // Should match catchall
		".myapp":       "app config",     // Should match catchall
		"data.json":    "{}",             // Should match catchall
		".dodot.toml":  "# config",       // Should be skipped
		".dodotignore": "*.tmp",          // Should be skipped by catchall
	}

	for filename, content := range testFiles {
		testutil.CreateFile(t, packDir, filename, content)
	}

	// Create a subdirectory that should match
	testutil.CreateDir(t, packDir, "subdir")
	testutil.CreateFile(t, filepath.Join(packDir, "subdir"), "nested.txt", "nested")

	// Create pack with default config (no pack config file)
	pack := types.Pack{
		Name:   "test-pack",
		Path:   packDir,
		Config: types.PackConfig{}, // Empty config, will use defaults
	}

	// Process triggers
	matches, err := core.ProcessPackTriggers(pack) //nolint:staticcheck // Testing deprecated function
	testutil.AssertNoError(t, err)

	// Build a map of matched files to their power-ups
	matchMap := make(map[string]string)
	for _, match := range matches {
		matchMap[match.Path] = match.PowerUpName
	}

	// Verify specific matchers worked
	testutil.AssertEqual(t, "symlink", matchMap[".vimrc"])
	testutil.AssertEqual(t, "symlink", matchMap[".bashrc"])
	testutil.AssertEqual(t, "homebrew", matchMap["Brewfile"])
	testutil.AssertEqual(t, "install_script", matchMap["install.sh"])

	// Verify catchall caught the remaining files
	testutil.AssertEqual(t, "symlink", matchMap["custom.conf"])
	testutil.AssertEqual(t, "symlink", matchMap["random.txt"])
	testutil.AssertEqual(t, "symlink", matchMap[".myapp"])
	testutil.AssertEqual(t, "symlink", matchMap["data.json"])
	testutil.AssertEqual(t, "symlink", matchMap["subdir"])

	// IMPORTANT: With flat scanning, nested.txt is NOT matched individually
	// It's part of the subdir/ directory which is processed as a unit
	if _, found := matchMap["subdir/nested.txt"]; found {
		t.Error("subdir/nested.txt should not be matched - flat scanning only processes top-level entries")
	}

	// Verify excluded files were not matched
	if _, found := matchMap[".dodot.toml"]; found {
		t.Error(".dodot.toml should not be matched")
	}
	if _, found := matchMap[".dodotignore"]; found {
		t.Error(".dodotignore should not be matched")
	}

	// Verify we got the expected number of matches
	expectedMatches := 9 // All top-level files/dirs except .dodot.toml and .dodotignore
	testutil.AssertEqual(t, expectedMatches, len(matches))
}

func TestProcessPackTriggers_CatchallWithOverrides(t *testing.T) {
	// Create test directory structure
	testDir := testutil.TempDir(t, "catchall-override-test")
	packDir := filepath.Join(testDir, "test-pack")
	testutil.CreateDir(t, testDir, "test-pack")

	// Create .dodot.toml with overrides
	packConfigContent := `
[[override]]
path = "custom.conf"
powerup = "shell_profile"

[[override]]
path = "data.json"
powerup = "template"
`
	testutil.CreateFile(t, packDir, ".dodot.toml", packConfigContent)

	// Create test files
	testFiles := map[string]string{
		"custom.conf": "custom config",  // Should be overridden to shell_profile
		"data.json":   "{}",             // Should be overridden to template
		"random.txt":  "random content", // Should match catchall
	}

	for filename, content := range testFiles {
		testutil.CreateFile(t, packDir, filename, content)
	}

	// Load the pack config
	configPath := filepath.Join(packDir, ".dodot.toml")
	packConfig, err := config.LoadPackConfig(configPath)
	testutil.AssertNoError(t, err)

	pack := types.Pack{
		Name:   "test-pack",
		Path:   packDir,
		Config: packConfig,
	}

	// Process triggers
	matches, err := core.ProcessPackTriggers(pack) //nolint:staticcheck // Testing deprecated function
	testutil.AssertNoError(t, err)

	// Build a map of matched files to their power-ups
	matchMap := make(map[string]string)
	for _, match := range matches {
		matchMap[match.Path] = match.PowerUpName
	}

	// Verify overrides took precedence over catchall
	testutil.AssertEqual(t, "shell_profile", matchMap["custom.conf"])
	testutil.AssertEqual(t, "template", matchMap["data.json"])

	// Verify catchall still caught the remaining file
	testutil.AssertEqual(t, "symlink", matchMap["random.txt"])
}
