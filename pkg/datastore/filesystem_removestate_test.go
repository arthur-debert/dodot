package datastore_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestFilesystemDataStore_RemoveState(t *testing.T) {
	tests := []struct {
		name        string
		packName    string
		handlerName string
		setupFunc   func(env *testutil.TestEnvironment)
		verifyFunc  func(t *testing.T, env *testutil.TestEnvironment, err error)
	}{
		{
			name:        "remove symlink handler state",
			packName:    "testpack",
			handlerName: "symlink",
			setupFunc: func(env *testutil.TestEnvironment) {
				// Create symlink state directory with content
				symlinkDir := env.Paths.PackHandlerDir("testpack", "symlinks")
				require.NoError(t, env.FS.MkdirAll(symlinkDir, 0755))
				require.NoError(t, env.FS.Symlink("source", filepath.Join(symlinkDir, "link")))
			},
			verifyFunc: func(t *testing.T, env *testutil.TestEnvironment, err error) {
				require.NoError(t, err)
				// Verify directory was removed
				symlinkDir := env.Paths.PackHandlerDir("testpack", "symlinks")
				_, statErr := env.FS.Stat(symlinkDir)
				assert.True(t, os.IsNotExist(statErr), "symlink state directory should be removed")
			},
		},
		{
			name:        "remove shell_profile handler state",
			packName:    "testpack",
			handlerName: "shell_profile",
			setupFunc: func(env *testutil.TestEnvironment) {
				// Create shell_profile state directory with content
				profileDir := env.Paths.PackHandlerDir("testpack", "shell_profiles")
				require.NoError(t, env.FS.MkdirAll(profileDir, 0755))
				require.NoError(t, env.FS.WriteFile(
					filepath.Join(profileDir, "profile.state"),
					[]byte("state content"),
					0644,
				))
			},
			verifyFunc: func(t *testing.T, env *testutil.TestEnvironment, err error) {
				require.NoError(t, err)
				// Verify directory was removed
				profileDir := env.Paths.PackHandlerDir("testpack", "shell_profiles")
				_, statErr := env.FS.Stat(profileDir)
				assert.True(t, os.IsNotExist(statErr), "shell_profile state directory should be removed")
			},
		},
		{
			name:        "remove path handler state",
			packName:    "testpack",
			handlerName: "path",
			setupFunc: func(env *testutil.TestEnvironment) {
				// Create path state directory with content
				pathDir := env.Paths.PackHandlerDir("testpack", "paths")
				require.NoError(t, env.FS.MkdirAll(pathDir, 0755))
				require.NoError(t, env.FS.WriteFile(
					filepath.Join(pathDir, "bin.state"),
					[]byte("path state"),
					0644,
				))
			},
			verifyFunc: func(t *testing.T, env *testutil.TestEnvironment, err error) {
				require.NoError(t, err)
				// Verify directory was removed
				pathDir := env.Paths.PackHandlerDir("testpack", "paths")
				_, statErr := env.FS.Stat(pathDir)
				assert.True(t, os.IsNotExist(statErr), "path state directory should be removed")
			},
		},
		{
			name:        "remove install handler state delegates to DeleteProvisioningState",
			packName:    "testpack",
			handlerName: "install",
			setupFunc: func(env *testutil.TestEnvironment) {
				// Create provisioning state in the correct location
				provDir := env.Paths.PackHandlerDir("testpack", "install")
				require.NoError(t, env.FS.MkdirAll(provDir, 0755))
				require.NoError(t, env.FS.WriteFile(
					filepath.Join(provDir, "install.sh.run"),
					[]byte("provisioning state"),
					0644,
				))
			},
			verifyFunc: func(t *testing.T, env *testutil.TestEnvironment, err error) {
				require.NoError(t, err)
				// Verify provisioning state was removed
				provDir := env.Paths.PackHandlerDir("testpack", "install")
				_, statErr := env.FS.Stat(provDir)
				assert.True(t, os.IsNotExist(statErr), "provisioning state should be removed")
			},
		},
		{
			name:        "remove homebrew handler state delegates to DeleteProvisioningState",
			packName:    "testpack",
			handlerName: "homebrew",
			setupFunc: func(env *testutil.TestEnvironment) {
				// Create provisioning state in the correct location
				provDir := env.Paths.PackHandlerDir("testpack", "homebrew")
				require.NoError(t, env.FS.MkdirAll(provDir, 0755))
				require.NoError(t, env.FS.WriteFile(
					filepath.Join(provDir, "Brewfile.run"),
					[]byte("brew state"),
					0644,
				))
			},
			verifyFunc: func(t *testing.T, env *testutil.TestEnvironment, err error) {
				require.NoError(t, err)
				// Verify provisioning state was removed
				provDir := env.Paths.PackHandlerDir("testpack", "homebrew")
				_, statErr := env.FS.Stat(provDir)
				assert.True(t, os.IsNotExist(statErr), "provisioning state should be removed")
			},
		},
		{
			name:        "unknown handler returns no error",
			packName:    "testpack",
			handlerName: "unknown",
			setupFunc:   func(env *testutil.TestEnvironment) {},
			verifyFunc: func(t *testing.T, env *testutil.TestEnvironment, err error) {
				assert.NoError(t, err, "unknown handler should not error")
			},
		},
		{
			name:        "remove non-existent state returns no error",
			packName:    "testpack",
			handlerName: "symlink",
			setupFunc: func(env *testutil.TestEnvironment) {
				// Don't create any state
			},
			verifyFunc: func(t *testing.T, env *testutil.TestEnvironment, err error) {
				assert.NoError(t, err, "removing non-existent state should not error")
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test environment
			env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
			defer env.Cleanup()

			// Create paths instance
			pathsInstance, err := paths.New(env.DotfilesRoot)
			require.NoError(t, err)

			// Create datastore
			ds := datastore.New(env.FS, pathsInstance)

			// Run setup
			tt.setupFunc(env)

			// Execute RemoveState
			err = ds.RemoveState(tt.packName, tt.handlerName)

			// Verify results
			tt.verifyFunc(t, env, err)
		})
	}
}

func TestFilesystemDataStore_RemoveState_Integration(t *testing.T) {
	t.Run("removes all handler types in sequence", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		packName := "multipack"
		// Create paths instance
		pathsInstance, err := paths.New(env.DotfilesRoot)
		require.NoError(t, err)

		// Create datastore
		ds := datastore.New(env.FS, pathsInstance)

		// Setup state for multiple handlers
		handlers := []string{"symlink", "shell_profile", "path", "install"}
		for _, handler := range handlers {
			var stateDir string
			switch handler {
			case "symlink":
				stateDir = env.Paths.PackHandlerDir(packName, "symlinks")
			case "shell_profile":
				stateDir = env.Paths.PackHandlerDir(packName, "shell_profiles")
			case "path":
				stateDir = env.Paths.PackHandlerDir(packName, "paths")
			case "install":
				stateDir = env.Paths.PackHandlerDir(packName, "install")
			}

			require.NoError(t, env.FS.MkdirAll(stateDir, 0755))
			require.NoError(t, env.FS.WriteFile(
				filepath.Join(stateDir, "test.state"),
				[]byte("test state"),
				0644,
			))
		}

		// Remove all states
		for _, handler := range handlers {
			err := ds.RemoveState(packName, handler)
			require.NoError(t, err, "should remove %s state", handler)
		}

		// Verify all states are gone
		// Check each handler's state directory
		for _, handler := range handlers {
			var stateDir string
			switch handler {
			case "symlink":
				stateDir = env.Paths.PackHandlerDir(packName, "symlinks")
			case "shell_profile":
				stateDir = env.Paths.PackHandlerDir(packName, "shell_profiles")
			case "path":
				stateDir = env.Paths.PackHandlerDir(packName, "paths")
			case "install":
				stateDir = env.Paths.PackHandlerDir(packName, "install")
			}

			_, err := env.FS.Stat(stateDir)
			assert.True(t, os.IsNotExist(err), "state directory for %s should be removed", handler)
		}
	})
}
