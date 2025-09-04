package commands_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/packs/commands"
	"github.com/arthur-debert/dodot/pkg/packs/orchestration"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOnCommand_Name(t *testing.T) {
	cmd := &commands.OnCommand{}
	assert.Equal(t, "on", cmd.Name())
}

func TestOnCommand_ExecuteForPack_Success(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Setup a pack with various handler types
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			"vimrc":            "set number",
			"vim/colors/theme": "colorscheme",
			"profile.sh":       "# shell config",
		},
		Rules: []testutil.Rule{
			{Type: "filename", Pattern: "vimrc", Handler: "symlink"},
			{Type: "filename", Pattern: "profile.sh", Handler: "shell"},
		},
	})

	pack := types.Pack{
		Name: "vim",
		Path: env.DotfilesRoot + "/vim",
	}

	cmd := &commands.OnCommand{
		NoProvision: false,
		Force:       false,
	}

	// Execute command
	result, err := cmd.ExecuteForPack(pack, orchestration.Options{
		DotfilesRoot: env.DotfilesRoot,
		DryRun:       true,
		FileSystem:   env.FS,
	})

	// Verify results
	require.NoError(t, err)
	assert.NotNil(t, result)
	assert.True(t, result.Success)
	assert.Equal(t, "vim", result.Pack.Name)
}

func TestOnCommand_ExecuteForPack_NoProvision(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Setup a pack with simple config files only
	env.SetupPack("config", testutil.PackConfig{
		Files: map[string]string{
			"gitconfig": "[user]\nname = Test",
			"vimrc":     "set number",
		},
		Rules: []testutil.Rule{
			{Type: "filename", Pattern: "gitconfig", Handler: "symlink"},
			{Type: "filename", Pattern: "vimrc", Handler: "symlink"},
		},
	})

	pack := types.Pack{
		Name: "config",
		Path: env.DotfilesRoot + "/config",
	}

	cmd := &commands.OnCommand{
		NoProvision: true, // Skip provisioning
		Force:       false,
	}

	// Execute command with dry run to avoid actual file operations
	result, err := cmd.ExecuteForPack(pack, orchestration.Options{
		DotfilesRoot: env.DotfilesRoot,
		DryRun:       true,
		FileSystem:   env.FS,
	})

	// Verify results
	require.NoError(t, err)
	assert.NotNil(t, result)
	assert.Equal(t, "config", result.Pack.Name)
	// In dry run mode, operations should succeed
	assert.True(t, result.Success)
}

func TestOnCommand_ExecuteForPack_InvalidPack(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Pack with invalid path
	pack := types.Pack{
		Name: "invalid",
		Path: "/nonexistent/path",
	}

	cmd := &commands.OnCommand{}

	// Execute command
	result, err := cmd.ExecuteForPack(pack, orchestration.Options{
		DotfilesRoot: env.DotfilesRoot,
		DryRun:       true,
		FileSystem:   env.FS,
	})

	// Should handle gracefully
	assert.NoError(t, err) // Command doesn't return error, just marks as unsuccessful
	assert.NotNil(t, result)
	assert.Equal(t, "invalid", result.Pack.Name)
}
