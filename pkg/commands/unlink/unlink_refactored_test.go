package unlink_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/unlink"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestUnlinkPacksV2_Clearable(t *testing.T) {
	tests := []struct {
		name        string
		setup       func(*testutil.TestEnvironment)
		packNames   []string
		dryRun      bool
		wantRemoved int
		wantError   bool
		checkResult func(*testing.T, *unlink.UnlinkResult, *testutil.TestEnvironment)
	}{
		{
			name: "unlink single pack with symlinks",
			setup: func(env *testutil.TestEnvironment) {
				// Create pack with symlink
				packDir := env.CreatePack("vim")
				testutil.CreateFile(t, packDir, ".vimrc", "set number")

				// Simulate symlink state
				stateDir := filepath.Join(env.DataDir(), "packs", "vim", "symlinks")
				require.NoError(t, os.MkdirAll(stateDir, 0755))

				// Create intermediate symlink
				intermediatePath := filepath.Join(stateDir, ".vimrc")
				sourcePath := filepath.Join(packDir, ".vimrc")
				testutil.CreateSymlink(t, sourcePath, intermediatePath)

				// Create user-facing symlink
				userPath := filepath.Join(env.Home(), ".vimrc")
				testutil.CreateSymlink(t, intermediatePath, userPath)
			},
			packNames:   []string{"vim"},
			dryRun:      false,
			wantRemoved: 2, // symlink + directory
			wantError:   false,
			checkResult: func(t *testing.T, result *unlink.UnlinkResult, env *testutil.TestEnvironment) {
				// Check symlink was removed
				userPath := filepath.Join(env.Home(), ".vimrc")
				_, err := os.Lstat(userPath)
				assert.True(t, os.IsNotExist(err), "User symlink should be removed")

				// Check state directory was removed
				stateDir := filepath.Join(env.DataDir(), "packs", "vim", "symlinks")
				_, err = os.Stat(stateDir)
				assert.True(t, os.IsNotExist(err), "State directory should be removed")
			},
		},
		{
			name: "unlink multiple packs with different handlers",
			setup: func(env *testutil.TestEnvironment) {
				// Pack 1: symlinks
				pack1Dir := env.CreatePack("vim")
				testutil.CreateFile(t, pack1Dir, ".vimrc", "set number")

				symlinksDir := filepath.Join(env.DataDir(), "packs", "vim", "symlinks")
				require.NoError(t, os.MkdirAll(symlinksDir, 0755))
				intermediatePath := filepath.Join(symlinksDir, ".vimrc")
				testutil.CreateSymlink(t, filepath.Join(pack1Dir, ".vimrc"), intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, filepath.Join(env.Home(), ".vimrc"))

				// Pack 2: path
				pack2Dir := env.CreatePack("tools")
				binDir := filepath.Join(pack2Dir, "bin")
				require.NoError(t, os.MkdirAll(binDir, 0755))

				pathDir := filepath.Join(env.DataDir(), "packs", "tools", "path")
				require.NoError(t, os.MkdirAll(pathDir, 0755))
				testutil.CreateSymlink(t, binDir, filepath.Join(pathDir, "bin"))
			},
			packNames:   []string{"vim", "tools"},
			dryRun:      false,
			wantRemoved: 4, // vim symlink + vim directory + tools path_state + tools directory
			wantError:   false,
			checkResult: func(t *testing.T, result *unlink.UnlinkResult, env *testutil.TestEnvironment) {
				assert.Len(t, result.Packs, 2)

				// Check vim pack
				for _, pack := range result.Packs {
					switch pack.Name {
					case "vim":
						assert.GreaterOrEqual(t, len(pack.RemovedItems), 2)
					case "tools":
						assert.GreaterOrEqual(t, len(pack.RemovedItems), 1)
					}
				}
			},
		},
		{
			name: "dry run preserves everything",
			setup: func(env *testutil.TestEnvironment) {
				packDir := env.CreatePack("test")
				testutil.CreateFile(t, packDir, ".testrc", "test config")

				stateDir := filepath.Join(env.DataDir(), "packs", "test", "symlinks")
				require.NoError(t, os.MkdirAll(stateDir, 0755))
				intermediatePath := filepath.Join(stateDir, ".testrc")
				testutil.CreateSymlink(t, filepath.Join(packDir, ".testrc"), intermediatePath)
				userPath := filepath.Join(env.Home(), ".testrc")
				testutil.CreateSymlink(t, intermediatePath, userPath)
			},
			packNames:   []string{"test"},
			dryRun:      true,
			wantRemoved: 2,
			wantError:   false,
			checkResult: func(t *testing.T, result *unlink.UnlinkResult, env *testutil.TestEnvironment) {
				assert.True(t, result.DryRun)

				// Check nothing was actually removed
				userPath := filepath.Join(env.Home(), ".testrc")
				_, err := os.Lstat(userPath)
				assert.NoError(t, err, "User symlink should still exist in dry run")

				stateDir := filepath.Join(env.DataDir(), "packs", "test", "symlinks")
				_, err = os.Stat(stateDir)
				assert.NoError(t, err, "State directory should still exist in dry run")
			},
		},
		{
			name: "skip packs without linking state",
			setup: func(env *testutil.TestEnvironment) {
				// Create pack with only provisioning state
				packDir := env.CreatePack("brew-only")
				testutil.CreateFile(t, packDir, "Brewfile", "brew 'git'")

				// Add only homebrew state (provisioning handler)
				brewDir := filepath.Join(env.DataDir(), "packs", "brew-only", "homebrew")
				require.NoError(t, os.MkdirAll(brewDir, 0755))
				testutil.CreateFile(t, brewDir, "brew-only_Brewfile.sentinel", "checksum")
			},
			packNames:   []string{"brew-only"},
			dryRun:      false,
			wantRemoved: 0,
			wantError:   false,
			checkResult: func(t *testing.T, result *unlink.UnlinkResult, env *testutil.TestEnvironment) {
				assert.Len(t, result.Packs, 1)
				assert.Empty(t, result.Packs[0].RemovedItems)

				// Provisioning state should remain
				brewDir := filepath.Join(env.DataDir(), "packs", "brew-only", "homebrew")
				_, err := os.Stat(brewDir)
				assert.NoError(t, err, "Homebrew state should be preserved")
			},
		},
		{
			name: "unlink all packs when none specified",
			setup: func(env *testutil.TestEnvironment) {
				// Create multiple packs
				pack1Dir := env.CreatePack("vim")
				testutil.CreateFile(t, pack1Dir, ".vimrc", "vim config")

				pack2Dir := env.CreatePack("zsh")
				testutil.CreateFile(t, pack2Dir, ".zshrc", "zsh config")

				// Add state only to vim
				stateDir := filepath.Join(env.DataDir(), "packs", "vim", "symlinks")
				require.NoError(t, os.MkdirAll(stateDir, 0755))
				intermediatePath := filepath.Join(stateDir, ".vimrc")
				testutil.CreateSymlink(t, filepath.Join(pack1Dir, ".vimrc"), intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, filepath.Join(env.Home(), ".vimrc"))
			},
			packNames:   []string{}, // Empty means all packs
			dryRun:      false,
			wantRemoved: 2, // Only vim has state
			wantError:   false,
			checkResult: func(t *testing.T, result *unlink.UnlinkResult, env *testutil.TestEnvironment) {
				// Should process all packs
				assert.GreaterOrEqual(t, len(result.Packs), 2)

				// Only vim should have removed items
				for _, pack := range result.Packs {
					switch pack.Name {
					case "vim":
						assert.NotEmpty(t, pack.RemovedItems)
					case "zsh":
						assert.Empty(t, pack.RemovedItems)
					}
				}
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

			opts := unlink.UnlinkPacksV2Options{
				DotfilesRoot: env.DotfilesRoot(),
				PackNames:    tt.packNames,
				DryRun:       tt.dryRun,
			}

			result, err := unlink.UnlinkPacksV2(opts)

			if tt.wantError {
				require.Error(t, err)
				return
			}

			require.NoError(t, err)
			require.NotNil(t, result)

			assert.Equal(t, tt.dryRun, result.DryRun)
			assert.Equal(t, tt.wantRemoved, result.TotalRemoved)

			if tt.checkResult != nil {
				tt.checkResult(t, result, env)
			}
		})
	}
}

func TestUnlinkPacks_BackwardCompatibility(t *testing.T) {
	// Test that the main UnlinkPacks function still works with the new implementation
	env := testutil.NewTestEnvironment(t, "backward-compat")
	defer env.Cleanup()

	// Create a simple pack
	packDir := env.CreatePack("test")
	testutil.CreateFile(t, packDir, ".testrc", "test")

	// Add symlink state
	stateDir := filepath.Join(env.DataDir(), "packs", "test", "symlinks")
	require.NoError(t, os.MkdirAll(stateDir, 0755))
	intermediatePath := filepath.Join(stateDir, ".testrc")
	testutil.CreateSymlink(t, filepath.Join(packDir, ".testrc"), intermediatePath)
	testutil.CreateSymlink(t, intermediatePath, filepath.Join(env.Home(), ".testrc"))

	// Use the original API
	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		DataDir:      env.DataDir(), // This field is ignored in v2
		PackNames:    []string{"test"},
		Force:        false,
		DryRun:       false,
	}

	result, err := unlink.UnlinkPacks(opts)
	require.NoError(t, err)
	require.NotNil(t, result)

	assert.Equal(t, 2, result.TotalRemoved)
	assert.Len(t, result.Packs, 1)
	assert.Equal(t, "test", result.Packs[0].Name)
}
