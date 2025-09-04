package commands_test

import (
	"fmt"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/packs/commands"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/utils"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetStatus_WithHandlerStatusChecking(t *testing.T) {
	// This test verifies that status checking is now delegated to handlers
	// Use isolated environment because install/homebrew handlers use os.Open
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	defer env.Cleanup()

	// Clean up any existing state from the datastore
	_ = env.DataStore.RemoveState("tools", "symlink")
	_ = env.DataStore.RemoveState("tools", "shell")
	_ = env.DataStore.RemoveState("tools", "path")
	_ = env.DataStore.RemoveState("tools", "install")
	_ = env.DataStore.RemoveState("tools", "homebrew")

	// Create a pack with different handler files
	env.SetupPack("tools", testutil.PackConfig{
		Files: map[string]string{
			"vimrc":      "\" Vim configuration",
			"aliases.sh": "alias ll='ls -la'",
			"bin/tool":   "#!/bin/bash\necho tool",
			"install.sh": "#!/bin/bash\necho installing",
			"Brewfile":   "brew 'git'",
		},
		Rules: []testutil.Rule{
			{Pattern: "vimrc", Handler: "symlink"},
			{Pattern: "aliases.sh", Handler: "shell"},
			{Pattern: "bin/*", Handler: "path"},
			{Pattern: "install.sh", Handler: "install"},
			{Pattern: "Brewfile", Handler: "homebrew"},
		},
	})

	// Get pack
	pack := types.Pack{
		Name: "tools",
		Path: filepath.Join(env.DotfilesRoot, "tools"),
	}

	// Create paths and datastore for this specific test
	pathsInstance, err := paths.New(env.DotfilesRoot)
	require.NoError(t, err)
	ds := datastore.New(env.FS, pathsInstance)

	// Get status - all handlers should report pending
	opts := commands.StatusOptions{
		Pack:       pack,
		DataStore:  ds,
		FileSystem: env.FS,
		Paths:      pathsInstance,
	}

	status, err := commands.GetStatus(opts)
	require.NoError(t, err)

	// Verify all files are pending
	assert.Len(t, status.Files, 5)
	for _, file := range status.Files {
		assert.Equal(t, commands.StatusStatePending, file.Status.State)

		// Verify handler-specific messages
		switch file.Handler {
		case "symlink":
			assert.Equal(t, "will be linked to $HOME/vimrc", file.Status.Message)
		case "shell":
			assert.Equal(t, "not sourced in shell", file.Status.Message)
		case "path":
			assert.Equal(t, "not in PATH", file.Status.Message)
		case "install":
			assert.Equal(t, "never run", file.Status.Message)
		case "homebrew":
			assert.Equal(t, "never installed", file.Status.Message)
		}
	}

	// Simulate linking the vimrc file by creating proper symlinks
	_, err = ds.CreateDataLink("tools", "symlink", filepath.Join(env.DotfilesRoot, "tools", "vimrc"))
	require.NoError(t, err)

	// Simulate running the install script
	// We need to calculate the actual checksum of the file
	installScript := filepath.Join(env.DotfilesRoot, "tools", "install.sh")
	checksum, checkErr := utils.CalculateFileChecksum(installScript)
	require.NoError(t, checkErr)
	sentinelName := fmt.Sprintf("install.sh-%s", checksum)
	err = ds.RunAndRecord("tools", "install", "echo 'done'", sentinelName)
	require.NoError(t, err)

	// Get status again
	status, err = commands.GetStatus(opts)
	require.NoError(t, err)

	// Verify updated statuses
	for _, file := range status.Files {
		switch file.Handler {
		case "symlink":
			assert.Equal(t, commands.StatusStateReady, file.Status.State)
			assert.Equal(t, "linked to $HOME/vimrc", file.Status.Message)
		case "install":
			assert.Equal(t, commands.StatusStateReady, file.Status.State)
			assert.Equal(t, "installed", file.Status.Message)
		default:
			// Others should still be pending
			assert.Equal(t, commands.StatusStatePending, file.Status.State)
		}
	}
}

func TestGetStatus_HandlerError(t *testing.T) {
	// Test that handler errors are properly propagated
	// Use isolated environment because install handler uses os.Open
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	defer env.Cleanup()

	// Create a pack with a file that will cause an error
	// (file exists in rules but not on filesystem)
	pack := types.Pack{
		Name: "broken",
		Path: filepath.Join(env.DotfilesRoot, "broken"),
	}

	// Create pack directory but not the file
	err := env.FS.MkdirAll(pack.Path, 0755)
	require.NoError(t, err)

	// Create a rules file that references a non-existent file
	rulesContent := `[[rules]]
pattern = "missing.sh"
handler = "install"`
	err = env.FS.WriteFile(filepath.Join(pack.Path, ".dodot.toml"), []byte(rulesContent), 0644)
	require.NoError(t, err)

	// This should cause the install handler to fail when calculating checksum
	pathsInstance, err := paths.New(env.DotfilesRoot)
	require.NoError(t, err)
	ds := datastore.New(env.FS, pathsInstance)

	opts := commands.StatusOptions{
		Pack:       pack,
		DataStore:  ds,
		FileSystem: env.FS,
		Paths:      pathsInstance,
	}

	// Should not error at the top level - individual file errors are captured
	status, err := commands.GetStatus(opts)
	require.NoError(t, err)

	// The pack should have an overall status based on files
	assert.Equal(t, "queue", status.Status) // No files matched
}
