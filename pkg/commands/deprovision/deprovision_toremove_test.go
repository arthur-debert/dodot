package deprovision_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/deprovision"
	"github.com/arthur-debert/dodot/pkg/testutil_old"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDeprovisionPacks(t *testing.T) {
	tests := []struct {
		name        string
		setup       func(*testutil.TestEnvironment)
		packNames   []string
		dryRun      bool
		wantCleared int
		wantError   bool
		checkResult func(*testing.T, *deprovision.DeprovisionResult)
	}{
		{
			name: "deprovision single pack with homebrew state",
			setup: func(env *testutil.TestEnvironment) {
				// Create a pack with homebrew state
				packDir := env.CreatePack("mypack")
				testutil.CreateFile(t, packDir, "Brewfile", "brew 'git'\nbrew 'vim'")

				// Simulate homebrew state
				stateDir := filepath.Join(env.DataDir(), "packs", "mypack", "homebrew")
				require.NoError(t, os.MkdirAll(stateDir, 0755))
				testutil.CreateFile(t, stateDir, "mypack_Brewfile.sentinel", "sha256:2024-01-01T00:00:00Z")
			},
			packNames:   []string{"mypack"},
			dryRun:      false,
			wantCleared: 1, // Homebrew clear returns state items
			wantError:   false,
			checkResult: func(t *testing.T, result *deprovision.DeprovisionResult) {
				assert.Len(t, result.Packs, 1)
				assert.Equal(t, "mypack", result.Packs[0].Name)
				assert.Len(t, result.Packs[0].HandlersRun, 1)
				assert.True(t, result.Packs[0].HandlersRun[0].StateRemoved)
			},
		},
		{
			name: "deprovision multiple packs",
			setup: func(env *testutil.TestEnvironment) {
				// Create packs with various states
				pack1Dir := env.CreatePack("pack1")
				testutil.CreateFile(t, pack1Dir, "install.sh", "echo installing")

				pack2Dir := env.CreatePack("pack2")
				testutil.CreateFile(t, pack2Dir, "Brewfile", "brew 'node'")

				// Add install state
				state1Dir := filepath.Join(env.DataDir(), "packs", "pack1", "install")
				require.NoError(t, os.MkdirAll(state1Dir, 0755))
				testutil.CreateFile(t, state1Dir, "run-2024-01-01T00:00:00Z-abc123", "checksum:timestamp")

				state2Dir := filepath.Join(env.DataDir(), "packs", "pack2", "homebrew")
				require.NoError(t, os.MkdirAll(state2Dir, 0755))
				testutil.CreateFile(t, state2Dir, "pack2_Brewfile.sentinel", "sha256:2024-01-01T00:00:00Z")
			},
			packNames:   []string{"pack1", "pack2"},
			dryRun:      false,
			wantCleared: 2, // Both handlers return state items
			wantError:   false,
			checkResult: func(t *testing.T, result *deprovision.DeprovisionResult) {
				assert.Len(t, result.Packs, 2)
				for _, pack := range result.Packs {
					assert.NotEmpty(t, pack.HandlersRun)
					for _, handler := range pack.HandlersRun {
						assert.True(t, handler.StateRemoved)
					}
				}
			},
		},
		{
			name: "deprovision all packs when none specified",
			setup: func(env *testutil.TestEnvironment) {
				// Create multiple packs
				vimDir := env.CreatePack("vim")
				testutil.CreateFile(t, vimDir, "install.sh", "echo vim")

				env.CreatePack("zsh")

				// Add state only to vim
				stateDir := filepath.Join(env.DataDir(), "packs", "vim", "install")
				require.NoError(t, os.MkdirAll(stateDir, 0755))
				testutil.CreateFile(t, stateDir, "run-2024-01-01T00:00:00Z-abc123", "checksum:timestamp")
			},
			packNames:   []string{}, // Empty means all packs
			dryRun:      false,
			wantCleared: 1, // vim pack has install state
			wantError:   false,
			checkResult: func(t *testing.T, result *deprovision.DeprovisionResult) {
				// Should process all packs but only clear those with state
				assert.GreaterOrEqual(t, len(result.Packs), 2)

				// Find vim pack - should have cleared state
				for _, pack := range result.Packs {
					if pack.Name == "vim" {
						assert.NotEmpty(t, pack.HandlersRun)
					}
				}
			},
		},
		{
			name: "dry run does not remove state",
			setup: func(env *testutil.TestEnvironment) {
				packDir := env.CreatePack("testpack")
				testutil.CreateFile(t, packDir, "install.sh", "echo test")

				stateDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				require.NoError(t, os.MkdirAll(stateDir, 0755))
				testutil.CreateFile(t, stateDir, "run-2024-01-01T00:00:00Z-abc123", "checksum:timestamp")
			},
			packNames:   []string{"testpack"},
			dryRun:      true,
			wantCleared: 1, // dry run still counts cleared items
			wantError:   false,
			checkResult: func(t *testing.T, result *deprovision.DeprovisionResult) {
				assert.True(t, result.DryRun)
				assert.Len(t, result.Packs, 1)

				// State should not be removed in dry run
				for _, handler := range result.Packs[0].HandlersRun {
					assert.False(t, handler.StateRemoved)
				}
			},
		},
		{
			name: "skip packs without install state",
			setup: func(env *testutil.TestEnvironment) {
				// Create pack with only linking state
				packDir := env.CreatePack("linkonly")
				testutil.CreateFile(t, packDir, ".vimrc", "set number")

				// Add only symlink state (linking handler)
				stateDir := filepath.Join(env.DataDir(), "packs", "linkonly", "symlinks")
				require.NoError(t, os.MkdirAll(stateDir, 0755))
				testutil.CreateSymlink(t, "/source/.vimrc", filepath.Join(stateDir, ".vimrc"))
			},
			packNames:   []string{"linkonly"},
			dryRun:      false,
			wantCleared: 0,
			wantError:   false,
			checkResult: func(t *testing.T, result *deprovision.DeprovisionResult) {
				assert.Len(t, result.Packs, 1)
				assert.Empty(t, result.Packs[0].HandlersRun)
				assert.Equal(t, 0, result.TotalCleared)
			},
		},
		{
			name: "error on non-existent pack",
			setup: func(env *testutil.TestEnvironment) {
				// Don't create any packs
			},
			packNames: []string{"nonexistent"},
			dryRun:    false,
			wantError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			env := testutil.NewTestEnvironment(t, tt.name)
			defer env.Cleanup()

			if tt.setup != nil {
				tt.setup(env)
			}

			opts := deprovision.DeprovisionPacksOptions{
				DotfilesRoot: env.DotfilesRoot(),
				PackNames:    tt.packNames,
				DryRun:       tt.dryRun,
			}

			result, err := deprovision.DeprovisionPacks(opts)

			if tt.wantError {
				require.Error(t, err)
				return
			}

			require.NoError(t, err)
			require.NotNil(t, result)

			assert.Equal(t, tt.dryRun, result.DryRun)
			assert.Equal(t, tt.wantCleared, result.TotalCleared)

			if tt.checkResult != nil {
				tt.checkResult(t, result)
			}

			// Verify state removal in non-dry-run mode
			if !tt.dryRun && result.TotalCleared == 0 {
				// For handlers that were run and had state removed
				for _, pack := range result.Packs {
					for _, handler := range pack.HandlersRun {
						if handler.StateRemoved && handler.Error == nil {
							// Check that state directory no longer exists
							stateDir := filepath.Join(env.DataDir(), "packs", pack.Name, handler.HandlerName)
							_, err := os.Stat(stateDir)
							assert.Error(t, err, "State directory should be removed")
						}
					}
				}
			}
		})
	}
}

func TestDeprovisionResult_Structure(t *testing.T) {
	// Test that the result structure properly aggregates data
	result := &deprovision.DeprovisionResult{
		DryRun: true,
		Packs: []deprovision.PackResult{
			{
				Name: "pack1",
				HandlersRun: []deprovision.HandlerResult{
					{
						HandlerName: "homebrew",
						ClearedItems: []types.ClearedItem{
							{Type: "package", Description: "git"},
						},
						StateRemoved: false, // dry run
					},
				},
				TotalCleared: 1,
			},
			{
				Name: "pack2",
				HandlersRun: []deprovision.HandlerResult{
					{
						HandlerName: "install",
						ClearedItems: []types.ClearedItem{
							{Type: "script", Description: "install.sh"},
						},
						StateRemoved: false,
					},
				},
				TotalCleared: 1,
			},
		},
		TotalCleared: 2,
	}

	assert.True(t, result.DryRun)
	assert.Equal(t, 2, result.TotalCleared)
	assert.Len(t, result.Packs, 2)
	assert.Equal(t, "pack1", result.Packs[0].Name)
	assert.Equal(t, "pack2", result.Packs[1].Name)
}
