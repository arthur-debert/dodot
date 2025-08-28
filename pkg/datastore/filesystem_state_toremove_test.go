package datastore_test

import (
	"fmt"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/datastore"
	datastoreTestutil "github.com/arthur-debert/dodot/pkg/datastore/testutil"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil_old"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDeleteProvisioningState(t *testing.T) {
	tests := []struct {
		name        string
		packName    string
		handlerName string
		setupFunc   func(env *testutil.TestEnvironment, fs types.FS)
		wantErr     bool
		errContains string
		checkFunc   func(t *testing.T, env *testutil.TestEnvironment, fs types.FS)
	}{
		{
			name:        "deletes install handler state",
			packName:    "testpack",
			handlerName: "install",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create install state
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				require.NoError(t, fs.MkdirAll(installDir, 0755))
				sentinelPath := filepath.Join(installDir, "run-2024-01-01T00:00:00Z-abc123")
				require.NoError(t, fs.WriteFile(sentinelPath, []byte("checksum:timestamp"), 0644))
			},
			checkFunc: func(t *testing.T, env *testutil.TestEnvironment, fs types.FS) {
				// Verify install directory is gone
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				_, err := fs.Stat(installDir)
				assert.True(t, testutil.IsNotExist(err), "install directory should be removed")
			},
		},
		{
			name:        "deletes homebrew handler state",
			packName:    "testpack",
			handlerName: "homebrew",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create homebrew state
				brewDir := filepath.Join(env.DataDir(), "packs", "testpack", "homebrew")
				require.NoError(t, fs.MkdirAll(brewDir, 0755))
				sentinelPath := filepath.Join(brewDir, "testpack_Brewfile.sentinel")
				require.NoError(t, fs.WriteFile(sentinelPath, []byte("sha256:2024-01-01T00:00:00Z"), 0644))
			},
			checkFunc: func(t *testing.T, env *testutil.TestEnvironment, fs types.FS) {
				// Verify homebrew directory is gone
				brewDir := filepath.Join(env.DataDir(), "packs", "testpack", "homebrew")
				_, err := fs.Stat(brewDir)
				assert.True(t, testutil.IsNotExist(err), "homebrew directory should be removed")
			},
		},
		{
			name:        "preserves linking handler state",
			packName:    "testpack",
			handlerName: "symlinks",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create symlinks state
				symlinksDir := filepath.Join(env.DataDir(), "packs", "testpack", "symlinks")
				require.NoError(t, fs.MkdirAll(symlinksDir, 0755))
				linkPath := filepath.Join(symlinksDir, ".vimrc")
				require.NoError(t, fs.Symlink("/source/vimrc", linkPath))
			},
			wantErr:     true,
			errContains: "cannot delete state for non-provisioning handler",
			checkFunc: func(t *testing.T, env *testutil.TestEnvironment, fs types.FS) {
				// Verify symlinks directory still exists
				symlinksDir := filepath.Join(env.DataDir(), "packs", "testpack", "symlinks")
				info, err := fs.Stat(symlinksDir)
				require.NoError(t, err)
				assert.True(t, info.IsDir())
			},
		},
		{
			name:        "handles non-existent directory gracefully",
			packName:    "testpack",
			handlerName: "install",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Don't create anything
			},
			checkFunc: func(t *testing.T, env *testutil.TestEnvironment, fs types.FS) {
				// Nothing to check
			},
		},
		{
			name:        "removes directory with multiple files",
			packName:    "testpack",
			handlerName: "install",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create multiple install runs
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				require.NoError(t, fs.MkdirAll(installDir, 0755))
				for i := 0; i < 3; i++ {
					sentinelPath := filepath.Join(installDir, fmt.Sprintf("run-%d", i))
					require.NoError(t, fs.WriteFile(sentinelPath, []byte("data"), 0644))
				}
			},
			checkFunc: func(t *testing.T, env *testutil.TestEnvironment, fs types.FS) {
				// Verify install directory is gone
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				_, err := fs.Stat(installDir)
				assert.True(t, testutil.IsNotExist(err))
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Set up test environment
			env := testutil.NewTestEnvironment(t, "test-delete-provisioning")
			fs := datastoreTestutil.NewMockFS()

			// Create paths instance
			p, err := paths.New(env.DotfilesRoot())
			require.NoError(t, err)

			// Create datastore
			dataStore := datastore.New(fs, p)

			if tt.setupFunc != nil {
				tt.setupFunc(env, fs)
			}

			err = dataStore.DeleteProvisioningState(tt.packName, tt.handlerName)

			if tt.wantErr {
				require.Error(t, err)
				if tt.errContains != "" {
					assert.Contains(t, err.Error(), tt.errContains)
				}
			} else {
				require.NoError(t, err)
			}

			if tt.checkFunc != nil {
				tt.checkFunc(t, env, fs)
			}
		})
	}
}

func TestGetProvisioningHandlers(t *testing.T) {
	tests := []struct {
		name      string
		packName  string
		setupFunc func(env *testutil.TestEnvironment, fs types.FS)
		want      []string
	}{
		{
			name:     "returns empty list for non-existent pack",
			packName: "nonexistent",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Don't create anything
			},
			want: []string{},
		},
		{
			name:     "returns empty list for pack with no provisioning handlers",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create only linking handlers
				symlinksDir := filepath.Join(env.DataDir(), "packs", "testpack", "symlinks")
				require.NoError(t, fs.MkdirAll(symlinksDir, 0755))
				pathDir := filepath.Join(env.DataDir(), "packs", "testpack", "path")
				require.NoError(t, fs.MkdirAll(pathDir, 0755))
			},
			want: []string{},
		},
		{
			name:     "returns install handler",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create install state
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				require.NoError(t, fs.MkdirAll(installDir, 0755))
				sentinelPath := filepath.Join(installDir, "run-sentinel")
				require.NoError(t, fs.WriteFile(sentinelPath, []byte("data"), 0644))
			},
			want: []string{"install"},
		},
		{
			name:     "returns homebrew handler",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create homebrew state
				brewDir := filepath.Join(env.DataDir(), "packs", "testpack", "homebrew")
				require.NoError(t, fs.MkdirAll(brewDir, 0755))
				sentinelPath := filepath.Join(brewDir, "Brewfile.sentinel")
				require.NoError(t, fs.WriteFile(sentinelPath, []byte("data"), 0644))
			},
			want: []string{"homebrew"},
		},
		{
			name:     "returns multiple provisioning handlers",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create both install and homebrew state
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				require.NoError(t, fs.MkdirAll(installDir, 0755))
				sentinelPath := filepath.Join(installDir, "run-sentinel")
				require.NoError(t, fs.WriteFile(sentinelPath, []byte("data"), 0644))

				brewDir := filepath.Join(env.DataDir(), "packs", "testpack", "homebrew")
				require.NoError(t, fs.MkdirAll(brewDir, 0755))
				brewSentinel := filepath.Join(brewDir, "Brewfile.sentinel")
				require.NoError(t, fs.WriteFile(brewSentinel, []byte("data"), 0644))
			},
			want: []string{"homebrew", "install"},
		},
		{
			name:     "ignores empty provisioning directories",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create empty install directory
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				require.NoError(t, fs.MkdirAll(installDir, 0755))
			},
			want: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Set up test environment
			env := testutil.NewTestEnvironment(t, "test-get-handlers")
			fs := datastoreTestutil.NewMockFS()

			// Create paths instance
			p, err := paths.New(env.DotfilesRoot())
			require.NoError(t, err)

			// Create datastore
			dataStore := datastore.New(fs, p)

			if tt.setupFunc != nil {
				tt.setupFunc(env, fs)
			}

			got, err := dataStore.GetProvisioningHandlers(tt.packName)
			require.NoError(t, err)

			// Sort for consistent comparison
			assert.ElementsMatch(t, tt.want, got)
		})
	}
}

func TestListProvisioningState(t *testing.T) {
	tests := []struct {
		name      string
		packName  string
		setupFunc func(env *testutil.TestEnvironment, fs types.FS)
		want      map[string][]string
	}{
		{
			name:     "returns empty map for non-existent pack",
			packName: "nonexistent",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Don't create anything
			},
			want: map[string][]string{},
		},
		{
			name:     "returns install handler files",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create install state with multiple runs
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				require.NoError(t, fs.MkdirAll(installDir, 0755))

				files := []string{
					"run-2024-01-01T00:00:00Z-abc123",
					"run-2024-01-02T00:00:00Z-def456",
				}
				for _, file := range files {
					path := filepath.Join(installDir, file)
					require.NoError(t, fs.WriteFile(path, []byte("data"), 0644))
				}
			},
			want: map[string][]string{
				"install": {
					"run-2024-01-01T00:00:00Z-abc123",
					"run-2024-01-02T00:00:00Z-def456",
				},
			},
		},
		{
			name:     "returns homebrew handler files",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create homebrew state
				brewDir := filepath.Join(env.DataDir(), "packs", "testpack", "homebrew")
				require.NoError(t, fs.MkdirAll(brewDir, 0755))

				files := []string{
					"testpack_Brewfile.sentinel",
					"testpack_Brewfile.dev.sentinel",
				}
				for _, file := range files {
					path := filepath.Join(brewDir, file)
					require.NoError(t, fs.WriteFile(path, []byte("data"), 0644))
				}
			},
			want: map[string][]string{
				"homebrew": {
					"testpack_Brewfile.dev.sentinel",
					"testpack_Brewfile.sentinel",
				},
			},
		},
		{
			name:     "returns multiple handlers",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create install state
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				require.NoError(t, fs.MkdirAll(installDir, 0755))
				installFile := filepath.Join(installDir, "run-sentinel")
				require.NoError(t, fs.WriteFile(installFile, []byte("data"), 0644))

				// Create homebrew state
				brewDir := filepath.Join(env.DataDir(), "packs", "testpack", "homebrew")
				require.NoError(t, fs.MkdirAll(brewDir, 0755))
				brewFile := filepath.Join(brewDir, "Brewfile.sentinel")
				require.NoError(t, fs.WriteFile(brewFile, []byte("data"), 0644))

				// Create symlinks state (should be ignored)
				symlinksDir := filepath.Join(env.DataDir(), "packs", "testpack", "symlinks")
				require.NoError(t, fs.MkdirAll(symlinksDir, 0755))
				linkPath := filepath.Join(symlinksDir, ".vimrc")
				require.NoError(t, fs.Symlink("/source/vimrc", linkPath))
			},
			want: map[string][]string{
				"install":  {"run-sentinel"},
				"homebrew": {"Brewfile.sentinel"},
			},
		},
		{
			name:     "ignores empty directories",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create empty provision directory
				provisionDir := filepath.Join(env.DataDir(), "packs", "testpack", "provision")
				require.NoError(t, fs.MkdirAll(provisionDir, 0755))
			},
			want: map[string][]string{},
		},
		{
			name:     "ignores subdirectories",
			packName: "testpack",
			setupFunc: func(env *testutil.TestEnvironment, fs types.FS) {
				// Create install state with a subdirectory
				installDir := filepath.Join(env.DataDir(), "packs", "testpack", "install")
				require.NoError(t, fs.MkdirAll(installDir, 0755))

				// Create a file
				file := filepath.Join(installDir, "run-sentinel")
				require.NoError(t, fs.WriteFile(file, []byte("data"), 0644))

				// Create a subdirectory (should be ignored)
				subdir := filepath.Join(installDir, "subdir")
				require.NoError(t, fs.MkdirAll(subdir, 0755))
			},
			want: map[string][]string{
				"install": {"run-sentinel"},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Set up test environment
			env := testutil.NewTestEnvironment(t, "test-list-state")
			fs := datastoreTestutil.NewMockFS()

			// Create paths instance
			p, err := paths.New(env.DotfilesRoot())
			require.NoError(t, err)

			// Create datastore
			dataStore := datastore.New(fs, p)

			if tt.setupFunc != nil {
				tt.setupFunc(env, fs)
			}

			got, err := dataStore.ListProvisioningState(tt.packName)
			require.NoError(t, err)

			// Compare maps
			assert.Equal(t, len(tt.want), len(got), "map length mismatch")
			for handler, wantFiles := range tt.want {
				gotFiles, ok := got[handler]
				assert.True(t, ok, "missing handler %s", handler)
				assert.ElementsMatch(t, wantFiles, gotFiles, "files mismatch for handler %s", handler)
			}
		})
	}
}
