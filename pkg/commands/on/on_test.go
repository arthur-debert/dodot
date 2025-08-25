package on

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/off"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOnPacks_EmptyPacks(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-empty")
	defer env.Cleanup()

	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{},
		DryRun:       false,
	}

	result, err := OnPacks(opts)
	require.NoError(t, err)
	assert.Empty(t, result.Packs)
	assert.Zero(t, result.TotalRestored)
	assert.Zero(t, result.TotalDeployed)
	assert.False(t, result.DryRun)
}

func TestOnPacks_NoOffState(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-no-off-state")
	defer env.Cleanup()

	// Create a pack
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")

	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       true, // Use dry run to avoid actual deployment
		Force:        false,
	}

	result, err := OnPacks(opts)
	require.NoError(t, err)
	require.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "mypack", packResult.Name)
	assert.False(t, packResult.WasOff)
	assert.False(t, packResult.StateRestored)
	assert.True(t, packResult.Redeployed) // Should re-deploy since no off-state exists
	assert.NotNil(t, packResult.ExecutionCtx)
	assert.NoError(t, packResult.Error)
}

func TestOnPacks_WithOffState_ForceRedeploy(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-force-redeploy")
	defer env.Cleanup()

	// Create a pack
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")

	// Create off-state to simulate pack being turned off
	p, err := paths.New(env.DotfilesRoot())
	require.NoError(t, err)

	offState := off.PackState{
		PackName: "mypack",
		Handlers: map[string]off.HandlerState{
			"symlink": {
				HandlerName:  "symlink",
				ClearedItems: []types.ClearedItem{},
				StateData:    make(map[string]interface{}),
			},
		},
		Confirmations: map[string]bool{},
		Version:       "1.0.0",
		TurnedOffAt:   "2024-01-01T00:00:00Z",
	}

	// Create off-state directory and file
	offStateDir := filepath.Join(env.DataDir(), "off-state")
	require.NoError(t, os.MkdirAll(offStateDir, 0755))
	data, err := json.MarshalIndent(offState, "", "  ")
	require.NoError(t, err)
	stateFile := filepath.Join(offStateDir, "mypack.json")
	require.NoError(t, os.WriteFile(stateFile, data, 0644))

	// Verify pack is considered "off"
	assert.True(t, off.IsPackOff(p, "mypack"))

	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       true,
		Force:        true, // Force re-deployment instead of state restoration
	}

	result, err := OnPacks(opts)
	require.NoError(t, err)
	require.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "mypack", packResult.Name)
	assert.True(t, packResult.WasOff)
	assert.False(t, packResult.StateRestored) // Force=true skips state restoration
	assert.True(t, packResult.Redeployed)
	assert.NotNil(t, packResult.ExecutionCtx)
	assert.NoError(t, packResult.Error)

	// In dry run, off-state should still exist
	assert.True(t, off.IsPackOff(p, "mypack"))
}

func TestOnPacks_WithOffState_StateRestoration(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-state-restoration")
	defer env.Cleanup()

	// Create a pack
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")

	// Create off-state
	p, err := paths.New(env.DotfilesRoot())
	require.NoError(t, err)

	offState := off.PackState{
		PackName: "mypack",
		Handlers: map[string]off.HandlerState{
			"symlink": {
				HandlerName:  "symlink",
				ClearedItems: []types.ClearedItem{},
				StateData:    make(map[string]interface{}),
			},
		},
		Confirmations: map[string]bool{},
		Version:       "1.0.0",
		TurnedOffAt:   "2024-01-01T00:00:00Z",
	}

	offStateDir := filepath.Join(env.DataDir(), "off-state")
	require.NoError(t, os.MkdirAll(offStateDir, 0755))
	data, err := json.MarshalIndent(offState, "", "  ")
	require.NoError(t, err)
	stateFile := filepath.Join(offStateDir, "mypack.json")
	require.NoError(t, os.WriteFile(stateFile, data, 0644))

	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       false,
		Force:        false, // Try state restoration first
	}

	result, err := OnPacks(opts)
	require.NoError(t, err)
	require.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "mypack", packResult.Name)
	assert.True(t, packResult.WasOff)

	// Currently, state restoration is not implemented, so it should fall back to re-deployment
	// When state restoration is implemented, this test would need to be updated
	assert.False(t, packResult.StateRestored) // Not implemented yet
	assert.True(t, packResult.Redeployed)     // Falls back to re-deployment
	assert.NotNil(t, packResult.ExecutionCtx)
	assert.NoError(t, packResult.Error)

	// Off-state file should be cleaned up after successful operation
	assert.False(t, off.IsPackOff(p, "mypack"))
}

func TestOnPacks_AutoDiscovery(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-auto-discovery")
	defer env.Cleanup()

	// Create multiple packs
	pack1Dir := env.CreatePack("pack1")
	pack2Dir := env.CreatePack("pack2")
	pack3Dir := env.CreatePack("pack3")
	testutil.CreateFile(t, pack1Dir, ".vimrc", "set number")
	testutil.CreateFile(t, pack2Dir, ".bashrc", "alias ll='ls -la'")
	testutil.CreateFile(t, pack3Dir, ".zshrc", "export PATH=$PATH:/usr/local/bin")

	// Create off-state for pack1 and pack2 (but not pack3)
	offStateDir := filepath.Join(env.DataDir(), "off-state")
	require.NoError(t, os.MkdirAll(offStateDir, 0755))

	for _, packName := range []string{"pack1", "pack2"} {
		offState := off.PackState{
			PackName:      packName,
			Handlers:      map[string]off.HandlerState{},
			Confirmations: map[string]bool{},
			Version:       "1.0.0",
			TurnedOffAt:   "2024-01-01T00:00:00Z",
		}
		data, err := json.MarshalIndent(offState, "", "  ")
		require.NoError(t, err)
		stateFile := filepath.Join(offStateDir, packName+".json")
		require.NoError(t, os.WriteFile(stateFile, data, 0644))
	}

	// Run on command with no pack names (should auto-discover off packs)
	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{}, // Empty = auto-discover
		DryRun:       true,
		Force:        false,
	}

	result, err := OnPacks(opts)
	require.NoError(t, err)

	// Should find pack1 and pack2 (but not pack3 since it's not off)
	assert.Len(t, result.Packs, 2)

	packNames := []string{result.Packs[0].Name, result.Packs[1].Name}
	assert.Contains(t, packNames, "pack1")
	assert.Contains(t, packNames, "pack2")
	assert.NotContains(t, packNames, "pack3")

	// All discovered packs should be marked as wasOff=true
	for _, packResult := range result.Packs {
		assert.True(t, packResult.WasOff)
	}
}

func TestOnPacks_NonExistentPack(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-nonexistent")
	defer env.Cleanup()

	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"nonexistent"},
		DryRun:       false,
	}

	result, err := OnPacks(opts)
	// Should return error for nonexistent pack
	require.Error(t, err)
	assert.Contains(t, err.Error(), "failed to discover packs")
	assert.Nil(t, result)
}

func TestFindOffPacks(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "find-off-packs")
	defer env.Cleanup()

	p, err := paths.New(env.DotfilesRoot())
	require.NoError(t, err)

	// Test with no off-state directory
	offPacks, err := findOffPacks(p)
	require.NoError(t, err)
	assert.Empty(t, offPacks)

	// Create off-state directory with some files
	offStateDir := filepath.Join(env.DataDir(), "off-state")
	require.NoError(t, os.MkdirAll(offStateDir, 0755))

	// Create state files for multiple packs
	testutil.CreateFile(t, filepath.Join(offStateDir, "pack1.json"), "", "{}")
	testutil.CreateFile(t, filepath.Join(offStateDir, "pack2.json"), "", "{}")
	testutil.CreateFile(t, filepath.Join(offStateDir, "not-a-pack.txt"), "", "ignore me")

	// Test finding off packs
	offPacks, err = findOffPacks(p)
	require.NoError(t, err)
	assert.Len(t, offPacks, 2)
	assert.Contains(t, offPacks, "pack1")
	assert.Contains(t, offPacks, "pack2")
	assert.NotContains(t, offPacks, "not-a-pack") // Should ignore non-.json files
}

func TestRemovePackOffState(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "remove-off-state")
	defer env.Cleanup()

	p, err := paths.New(env.DotfilesRoot())
	require.NoError(t, err)

	// Create off-state file
	offStateDir := filepath.Join(env.DataDir(), "off-state")
	require.NoError(t, os.MkdirAll(offStateDir, 0755))
	stateFile := filepath.Join(offStateDir, "mypack.json")
	testutil.CreateFile(t, stateFile, "", "{}")

	// Verify file exists
	assert.FileExists(t, stateFile)
	assert.True(t, off.IsPackOff(p, "mypack"))

	// Remove off-state
	err = removePackOffState(p, "mypack")
	require.NoError(t, err)

	// Verify file is gone
	assert.NoFileExists(t, stateFile)
	assert.False(t, off.IsPackOff(p, "mypack"))

	// Test removing non-existent state (should not error)
	err = removePackOffState(p, "nonexistent")
	require.NoError(t, err)
}
