package symlink

import (
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestSymlinkHandler_EnvironmentExpansion tests environment variable expansion
// This is an integration test because it manipulates environment variables
func TestSymlinkHandler_EnvironmentExpansion(t *testing.T) {
	oldCustomDir := os.Getenv("CUSTOM_DIR")
	require.NoError(t, os.Setenv("CUSTOM_DIR", "/expanded/path"))
	defer func() {
		if oldCustomDir != "" {
			require.NoError(t, os.Setenv("CUSTOM_DIR", oldCustomDir))
		} else {
			require.NoError(t, os.Unsetenv("CUSTOM_DIR"))
		}
	}()

	powerUp := NewSymlinkHandler()

	matches := []types.TriggerMatch{
		{
			TriggerName:  "filename",
			Pack:         "test",
			Path:         "file.txt",
			AbsolutePath: "/dotfiles/test/file.txt",
			HandlerName:  "symlink",
			HandlerOptions: map[string]interface{}{
				"target": "$CUSTOM_DIR/configs",
			},
		},
	}

	actions, err := powerUp.Process(matches)
	require.NoError(t, err)
	require.Len(t, actions, 1)

	assert.Equal(t, "/expanded/path/configs/file.txt", actions[0].Target)
}

// TestSymlinkHandler_FactoryRegistration tests registry integration
// This is an integration test because it uses the global registry
func TestSymlinkHandler_FactoryRegistration(t *testing.T) {
	// Test that the factory is registered
	factory, err := registry.GetHandlerFactory(SymlinkHandlerName)
	require.NoError(t, err)
	require.NotNil(t, factory)

	// Test factory creates power-up correctly
	powerUp, err := factory(nil)
	require.NoError(t, err)
	require.NotNil(t, powerUp)

	assert.Equal(t, SymlinkHandlerName, powerUp.Name())
}
