package orchestration_test

import (
	"errors"
	"testing"

	"github.com/arthur-debert/dodot/pkg/packs/orchestration"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// mockCommand is a test command implementation
type mockCommand struct {
	name          string
	executeFunc   func(pack types.Pack, opts orchestration.Options) (*orchestration.PackResult, error)
	executedPacks []string
}

func (m *mockCommand) Name() string {
	return m.name
}

func (m *mockCommand) ExecuteForPack(pack types.Pack, opts orchestration.Options) (*orchestration.PackResult, error) {
	m.executedPacks = append(m.executedPacks, pack.Name)
	if m.executeFunc != nil {
		return m.executeFunc(pack, opts)
	}
	return &orchestration.PackResult{
		Pack:    pack,
		Success: true,
	}, nil
}

func TestExecute_Success(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Setup test packs
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			"vimrc": "set number",
		},
	})
	env.SetupPack("zsh", testutil.PackConfig{
		Files: map[string]string{
			"zshrc": "export PATH",
		},
	})

	// Create mock command
	cmd := &mockCommand{
		name: "test",
	}

	// Execute pipeline
	result, err := orchestration.Execute(cmd, []string{"vim", "zsh"}, orchestration.Options{
		DotfilesRoot: env.DotfilesRoot,
		DryRun:       false,
		FileSystem:   env.FS,
	})

	// Verify results
	require.NoError(t, err)
	assert.Equal(t, "test", result.Command)
	assert.Equal(t, 2, result.TotalPacks)
	assert.Equal(t, 2, result.SuccessfulPacks)
	assert.Equal(t, 0, result.FailedPacks)
	assert.Len(t, result.PackResults, 2)
	assert.Equal(t, []string{"vim", "zsh"}, cmd.executedPacks)
}

func TestExecute_PartialFailure(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Setup test packs
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			"vimrc": "set number",
		},
	})
	env.SetupPack("zsh", testutil.PackConfig{
		Files: map[string]string{
			"zshrc": "export PATH",
		},
	})

	// Create mock command that fails for zsh
	cmd := &mockCommand{
		name: "test",
		executeFunc: func(pack types.Pack, opts orchestration.Options) (*orchestration.PackResult, error) {
			if pack.Name == "zsh" {
				return &orchestration.PackResult{
					Pack:    pack,
					Success: false,
					Error:   errors.New("zsh failed"),
				}, errors.New("zsh failed")
			}
			return &orchestration.PackResult{
				Pack:    pack,
				Success: true,
			}, nil
		},
	}

	// Execute pipeline
	result, err := orchestration.Execute(cmd, []string{"vim", "zsh"}, orchestration.Options{
		DotfilesRoot: env.DotfilesRoot,
		DryRun:       false,
		FileSystem:   env.FS,
	})

	// Verify results
	require.NoError(t, err) // Pipeline itself doesn't fail
	assert.Equal(t, 2, result.TotalPacks)
	assert.Equal(t, 1, result.SuccessfulPacks)
	assert.Equal(t, 1, result.FailedPacks)
	assert.NotNil(t, result.Error)
	assert.Contains(t, result.Error.Error(), "1 pack(s) failed")
}

func TestExecute_AllPacks(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Setup test packs
	env.SetupPack("vim", testutil.PackConfig{Files: map[string]string{"vimrc": ""}})
	env.SetupPack("zsh", testutil.PackConfig{Files: map[string]string{"zshrc": ""}})
	env.SetupPack("git", testutil.PackConfig{Files: map[string]string{"gitconfig": ""}})

	// Create mock command
	cmd := &mockCommand{name: "test"}

	// Execute pipeline with empty pack names (all packs)
	result, err := orchestration.Execute(cmd, []string{}, orchestration.Options{
		DotfilesRoot: env.DotfilesRoot,
		FileSystem:   env.FS,
	})

	// Verify results
	require.NoError(t, err)
	assert.Equal(t, 3, result.TotalPacks)
	assert.Equal(t, 3, result.SuccessfulPacks)
	assert.Len(t, cmd.executedPacks, 3)
}

func TestExecute_InvalidPack(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Setup only one pack
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{"vimrc": ""},
	})

	// Create mock command
	cmd := &mockCommand{name: "test"}

	// Execute pipeline with non-existent pack
	_, err := orchestration.Execute(cmd, []string{"nonexistent"}, orchestration.Options{
		DotfilesRoot: env.DotfilesRoot,
		FileSystem:   env.FS,
	})

	// Should fail during pack discovery
	require.Error(t, err)
	assert.Contains(t, err.Error(), "pack(s) not found")
}

func TestExecuteSingle_Success(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Create a pack directly
	pack := types.Pack{
		Name: "vim",
		Path: env.DotfilesRoot + "/vim",
	}

	// Create mock command
	cmd := &mockCommand{
		name: "test",
		executeFunc: func(p types.Pack, opts orchestration.Options) (*orchestration.PackResult, error) {
			return &orchestration.PackResult{
				Pack:                  p,
				Success:               true,
				CommandSpecificResult: "test data",
			}, nil
		},
	}

	// Execute single pack
	result, err := orchestration.ExecuteSingle(cmd, pack, orchestration.Options{
		DotfilesRoot: env.DotfilesRoot,
		DryRun:       true,
	})

	// Verify results
	require.NoError(t, err)
	assert.True(t, result.Success)
	assert.Equal(t, "vim", result.Pack.Name)
	assert.Equal(t, "test data", result.CommandSpecificResult)
}
