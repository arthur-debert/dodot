package status

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestPowerUpChecker_NewPowerUpChecker(t *testing.T) {
	fs := newTestFileSystem()
	pc := NewPowerUpChecker(fs)

	assert.NotNil(t, pc)
	assert.NotNil(t, pc.fs)
	assert.NotNil(t, pc.checkers)

	// Verify all expected checkers are registered
	expectedCheckers := []string{"symlink", "shell_profile", "add_path", "homebrew"}
	for _, name := range expectedCheckers {
		_, exists := pc.checkers[name]
		assert.True(t, exists, "Expected checker for %s to be registered", name)
	}
}

func TestPowerUpChecker_CheckOperationStatus_UnknownPowerUp(t *testing.T) {
	fs := newTestFileSystem()
	pc := NewPowerUpChecker(fs)

	op := &types.Operation{
		Source:  "/test/source",
		PowerUp: "unknown_powerup",
	}

	status, err := pc.CheckOperationStatus(op)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, "/test/source", status.Path)
	assert.Equal(t, "unknown_powerup", status.PowerUp)
	assert.Equal(t, types.StatusUnknown, status.Status)
	assert.Contains(t, status.Message, "No status checker available")
}

func TestPowerUpChecker_CheckOperationStatus_KnownPowerUp(t *testing.T) {
	fs := newTestFileSystem()
	pc := NewPowerUpChecker(fs)

	// Create a simple symlink operation
	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/test/source",
		Target:  "/test/target",
		PowerUp: "symlink",
	}

	status, err := pc.CheckOperationStatus(op)
	require.NoError(t, err)
	require.NotNil(t, status)

	// Should get a valid status from the symlink checker
	assert.Equal(t, "/test/target", status.Path)
	assert.Equal(t, "symlink", status.PowerUp)
	assert.NotNil(t, status.Metadata)
}
