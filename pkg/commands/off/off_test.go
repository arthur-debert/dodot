package off

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOffPacks_EmptyPacks(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "off-empty")
	defer env.Cleanup()

	opts := OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{},
		DryRun:       false,
	}

	result, err := OffPacks(opts)
	require.NoError(t, err)
	assert.Empty(t, result.Packs)
	assert.Zero(t, result.TotalCleared)
	assert.False(t, result.DryRun)
}

func TestOffPacks_NoState(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "off-nostate")
	defer env.Cleanup()

	// Create a pack with no deployment state
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")

	opts := OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       false,
	}

	result, err := OffPacks(opts)
	require.NoError(t, err)
	require.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "mypack", packResult.Name)
	assert.Empty(t, packResult.HandlersRun)
	assert.Zero(t, packResult.TotalCleared)
	assert.False(t, packResult.StateStored)
	assert.NoError(t, packResult.Error)
}

func TestOffPacks_WithSymlinkState(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "off-symlinks")
	defer env.Cleanup()

	// Create pack
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")
	testutil.CreateFile(t, packDir, ".bashrc", "alias ll='ls -la'")

	// Simulate deployed symlink state
	stateDir := filepath.Join(env.DataDir(), "packs", "mypack", "symlinks")
	require.NoError(t, os.MkdirAll(stateDir, 0755))

	// Create intermediate symlinks (what datastore creates)
	vimrcIntermediate := filepath.Join(stateDir, ".vimrc")
	bashrcIntermediate := filepath.Join(stateDir, ".bashrc")
	testutil.CreateSymlink(t, filepath.Join(packDir, ".vimrc"), vimrcIntermediate)
	testutil.CreateSymlink(t, filepath.Join(packDir, ".bashrc"), bashrcIntermediate)

	// Create user-facing symlinks (what user sees)
	testutil.CreateSymlink(t, vimrcIntermediate, filepath.Join(env.Home(), ".vimrc"))
	testutil.CreateSymlink(t, bashrcIntermediate, filepath.Join(env.Home(), ".bashrc"))

	tests := []struct {
		name    string
		dryRun  bool
		wantDry bool
	}{
		{
			name:    "dry run",
			dryRun:  true,
			wantDry: true,
		},
		{
			name:    "actual run",
			dryRun:  false,
			wantDry: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			opts := OffPacksOptions{
				DotfilesRoot: env.DotfilesRoot(),
				PackNames:    []string{"mypack"},
				DryRun:       tt.dryRun,
			}

			result, err := OffPacks(opts)
			require.NoError(t, err)
			require.Len(t, result.Packs, 1)
			assert.Equal(t, tt.wantDry, result.DryRun)

			packResult := result.Packs[0]
			assert.Equal(t, "mypack", packResult.Name)
			assert.NoError(t, packResult.Error)

			// Should have symlink handler result
			require.Len(t, packResult.HandlersRun, 1)
			handlerResult := packResult.HandlersRun[0]
			assert.Equal(t, "symlink", handlerResult.HandlerName)
			assert.True(t, handlerResult.StateRemoved) // Handler cleared user-facing items
			assert.NotEmpty(t, handlerResult.ClearedItems)

			if tt.dryRun {
				// Dry run: state should indicate it WOULD be stored, symlinks should still exist
				assert.True(t, packResult.StateStored) // Would be stored if not dry run
				assert.FileExists(t, filepath.Join(env.Home(), ".vimrc"))
				assert.FileExists(t, filepath.Join(env.Home(), ".bashrc"))
			} else {
				// Actual run: state should be stored, symlinks should be removed
				assert.True(t, packResult.StateStored)

				// Check that off-state file was created
				stateFile := filepath.Join(env.DataDir(), "off-state", "mypack.json")
				assert.FileExists(t, stateFile)

				// Verify state file content
				data, err := os.ReadFile(stateFile)
				require.NoError(t, err)

				var state PackState
				require.NoError(t, json.Unmarshal(data, &state))
				assert.Equal(t, "mypack", state.PackName)
				assert.Contains(t, state.Handlers, "symlink")
				assert.Equal(t, offStateVersion, state.Version)

				// User-facing symlinks should be removed
				assert.NoFileExists(t, filepath.Join(env.Home(), ".vimrc"))
				assert.NoFileExists(t, filepath.Join(env.Home(), ".bashrc"))
			}
		})
	}
}

func TestOffPacks_WithHomebrewState(t *testing.T) {
	// Skip if homebrew not available
	if testing.Short() {
		t.Skip("Skipping homebrew test in short mode")
	}

	env := testutil.NewTestEnvironment(t, "off-homebrew")
	defer env.Cleanup()

	// Create pack with Brewfile
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, "Brewfile", `brew "git"
brew "vim"`)

	// Simulate homebrew deployment state
	stateDir := filepath.Join(env.DataDir(), "packs", "mypack", "homebrew")
	require.NoError(t, os.MkdirAll(stateDir, 0755))

	// Create sentinel file (indicates Brewfile was processed)
	sentinelFile := filepath.Join(stateDir, "mypack_Brewfile.sentinel")
	testutil.CreateFile(t, sentinelFile, "", "sha256:2024-01-01T00:00:00Z")

	// Test without DODOT_HOMEBREW_UNINSTALL (should not generate confirmations)
	t.Run("no uninstall enabled", func(t *testing.T) {
		opts := OffPacksOptions{
			DotfilesRoot: env.DotfilesRoot(),
			PackNames:    []string{"mypack"},
			DryRun:       false,
		}

		result, err := OffPacks(opts)
		require.NoError(t, err)
		require.Len(t, result.Packs, 1)

		packResult := result.Packs[0]
		assert.Equal(t, "mypack", packResult.Name)
		assert.NoError(t, packResult.Error)

		// Should have homebrew handler result
		require.Len(t, packResult.HandlersRun, 1)
		handlerResult := packResult.HandlersRun[0]
		assert.Equal(t, "homebrew", handlerResult.HandlerName)
		assert.True(t, handlerResult.StateRemoved)
		assert.Empty(t, handlerResult.ConfirmationIDs) // No confirmations without DODOT_HOMEBREW_UNINSTALL
		assert.NotEmpty(t, handlerResult.ClearedItems)
	})

	// Test with DODOT_HOMEBREW_UNINSTALL enabled (would generate confirmations in interactive mode)
	// Note: In tests, we can't easily test interactive confirmation dialogs,
	// so we primarily test the dry run behavior and state management
	t.Run("uninstall enabled dry run", func(t *testing.T) {
		t.Setenv("DODOT_HOMEBREW_UNINSTALL", "true")

		opts := OffPacksOptions{
			DotfilesRoot: env.DotfilesRoot(),
			PackNames:    []string{"mypack"},
			DryRun:       true, // Dry run avoids interactive confirmation dialog
		}

		result, err := OffPacks(opts)
		require.NoError(t, err)
		require.Len(t, result.Packs, 1)

		packResult := result.Packs[0]
		assert.Equal(t, "mypack", packResult.Name)
		assert.NoError(t, packResult.Error)
		assert.False(t, packResult.StateStored) // Dry run doesn't store state

		// Should have homebrew handler result
		require.Len(t, packResult.HandlersRun, 1)
		handlerResult := packResult.HandlersRun[0]
		assert.Equal(t, "homebrew", handlerResult.HandlerName)
		assert.True(t, handlerResult.StateRemoved)
		assert.NotEmpty(t, handlerResult.ClearedItems)
	})
}

func TestOffPacks_MultipleHandlers(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "off-multiple")
	defer env.Cleanup()

	// Create pack with multiple handler types
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")
	testutil.CreateFile(t, packDir, "Brewfile", `brew "git"`)
	testutil.CreateFile(t, packDir, "install.sh", "#!/bin/bash\necho 'installed'")

	binDir := filepath.Join(packDir, "bin")
	require.NoError(t, os.MkdirAll(binDir, 0755))
	testutil.CreateFile(t, binDir, "myscript", "#!/bin/bash\necho 'hello'")

	// Simulate deployed state for multiple handlers

	// Symlink handler state
	symlinkStateDir := filepath.Join(env.DataDir(), "packs", "mypack", "symlinks")
	require.NoError(t, os.MkdirAll(symlinkStateDir, 0755))
	vimrcIntermediate := filepath.Join(symlinkStateDir, ".vimrc")
	testutil.CreateSymlink(t, filepath.Join(packDir, ".vimrc"), vimrcIntermediate)
	testutil.CreateSymlink(t, vimrcIntermediate, filepath.Join(env.Home(), ".vimrc"))

	// Homebrew handler state
	homebrewStateDir := filepath.Join(env.DataDir(), "packs", "mypack", "homebrew")
	require.NoError(t, os.MkdirAll(homebrewStateDir, 0755))
	sentinelFile := filepath.Join(homebrewStateDir, "mypack_Brewfile.sentinel")
	testutil.CreateFile(t, sentinelFile, "", "sha256:2024-01-01T00:00:00Z")

	// Provision handler state
	provisionStateDir := filepath.Join(env.DataDir(), "packs", "mypack", "provision")
	require.NoError(t, os.MkdirAll(provisionStateDir, 0755))
	runRecord := filepath.Join(provisionStateDir, "run-2024-01-01T00:00:00Z-abc123")
	testutil.CreateFile(t, runRecord, "", "sha256:xyz")

	// PATH handler state
	pathStateDir := filepath.Join(env.DataDir(), "packs", "mypack", "path")
	require.NoError(t, os.MkdirAll(pathStateDir, 0755))
	pathEntry := filepath.Join(pathStateDir, "bin")
	testutil.CreateSymlink(t, binDir, pathEntry)

	opts := OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       false,
	}

	result, err := OffPacks(opts)
	require.NoError(t, err)
	require.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "mypack", packResult.Name)
	assert.NoError(t, packResult.Error)
	assert.True(t, packResult.StateStored)

	// Should have results for all handlers with state
	assert.Len(t, packResult.HandlersRun, 4) // symlink, homebrew, provision, path

	handlerNames := make([]string, len(packResult.HandlersRun))
	for i, hr := range packResult.HandlersRun {
		handlerNames[i] = hr.HandlerName
		assert.True(t, hr.StateRemoved)
		assert.NotEmpty(t, hr.ClearedItems)
		assert.NoError(t, hr.Error)
	}

	// Check that all expected handlers are present (order may vary)
	assert.Contains(t, handlerNames, "symlink")
	assert.Contains(t, handlerNames, "homebrew")
	assert.Contains(t, handlerNames, "provision")
	assert.Contains(t, handlerNames, "path")

	// Verify off-state file contains all handlers
	stateFile := filepath.Join(env.DataDir(), "off-state", "mypack.json")
	assert.FileExists(t, stateFile)

	data, err := os.ReadFile(stateFile)
	require.NoError(t, err)

	var state PackState
	require.NoError(t, json.Unmarshal(data, &state))
	assert.Equal(t, "mypack", state.PackName)
	assert.Len(t, state.Handlers, 4)
	assert.Contains(t, state.Handlers, "symlink")
	assert.Contains(t, state.Handlers, "homebrew")
	assert.Contains(t, state.Handlers, "provision")
	assert.Contains(t, state.Handlers, "path")
}

func TestOffPacks_NonExistentPack(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "off-nonexistent")
	defer env.Cleanup()

	opts := OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"nonexistent"},
		DryRun:       false,
	}

	result, err := OffPacks(opts)
	// Should return error for nonexistent pack
	require.Error(t, err)
	assert.Contains(t, err.Error(), "failed to discover packs")
	assert.Nil(t, result)
}

func TestLoadPackState(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "load-state")
	defer env.Cleanup()

	// Create off-state directory and file
	offStateDir := filepath.Join(env.DataDir(), "off-state")
	require.NoError(t, os.MkdirAll(offStateDir, 0755))

	state := PackState{
		PackName: "mypack",
		Handlers: map[string]HandlerState{
			"symlink": {
				HandlerName:  "symlink",
				ClearedItems: []types.ClearedItem{},
				StateData:    make(map[string]interface{}),
			},
		},
		Confirmations: map[string]bool{},
		Version:       offStateVersion,
		TurnedOffAt:   "2024-01-01T00:00:00Z",
	}

	data, err := json.MarshalIndent(state, "", "  ")
	require.NoError(t, err)

	stateFile := filepath.Join(offStateDir, "mypack.json")
	require.NoError(t, os.WriteFile(stateFile, data, 0644))

	// Create paths instance
	p, err := paths.New(env.DotfilesRoot())
	require.NoError(t, err)

	// Test loading existing state
	loadedState, err := LoadPackState(p, "mypack")
	require.NoError(t, err)
	assert.Equal(t, "mypack", loadedState.PackName)
	assert.Equal(t, offStateVersion, loadedState.Version)
	assert.Contains(t, loadedState.Handlers, "symlink")

	// Test loading non-existent state
	_, err = LoadPackState(p, "nonexistent")
	require.Error(t, err)
	assert.Contains(t, err.Error(), "is not turned off")
}

func TestIsPackOff(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "is-pack-off")
	defer env.Cleanup()

	// Create paths instance
	p, err := paths.New(env.DotfilesRoot())
	require.NoError(t, err)

	// Test pack that is not off
	assert.False(t, IsPackOff(p, "mypack"))

	// Create off-state file
	offStateDir := filepath.Join(env.DataDir(), "off-state")
	require.NoError(t, os.MkdirAll(offStateDir, 0755))
	stateFile := filepath.Join(offStateDir, "mypack.json")
	testutil.CreateFile(t, stateFile, "", "{}")

	// Test pack that is off
	assert.True(t, IsPackOff(p, "mypack"))
}
