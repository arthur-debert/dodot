//go:build integration
// +build integration

package install_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestInstallPowerUpGeneratesExecuteOperation(t *testing.T) {
	t.Run("convertInstallActionWithContext should generate execute operation", func(t *testing.T) {
		// Create a test install action
		action := types.Action{
			Type:   types.ActionTypeInstall,
			Source: "/dotfiles/testpack/install.sh",
			Target: "", // Install actions don't have targets
			Metadata: map[string]interface{}{
				"pack":     "testpack",
				"checksum": "abc123",
			},
		}

		// Create execution context
		ctx := core.NewExecutionContext(false)

		// Convert action to operations
		operations, err := core.GetFileOperationsWithContext([]types.Action{action}, ctx)
		require.NoError(t, err)

		// Check that we have operations
		require.NotEmpty(t, operations, "Should generate operations for install action")

		// Look for execute operation
		hasExecuteOp := false
		hasSentinelOp := false

		for _, op := range operations {
			t.Logf("Operation type: %s, description: %s", op.Type, op.Description)

			if op.Type == types.OperationWriteFile && op.Description == "Create install sentinel for testpack" {
				hasSentinelOp = true
			}

			// Check if there's any operation that would execute the script
			// Currently looking for a hypothetical OperationExecute type
			if string(op.Type) == "execute" || op.Description == "Execute install script for testpack" {
				hasExecuteOp = true
			}
		}

		assert.True(t, hasSentinelOp, "Should generate sentinel file operation")
		assert.True(t, hasExecuteOp, "Should generate execute operation for install script - this is the bug!")
	})

	t.Run("install operations include all necessary steps", func(t *testing.T) {
		// Create test action
		action := types.Action{
			Type:   types.ActionTypeInstall,
			Source: "/path/to/install.sh",
			Target: "",
			Metadata: map[string]interface{}{
				"pack":     "testpack",
				"checksum": "abc123",
			},
		}

		// Create execution context
		ctx := core.NewExecutionContext(false)

		// Convert action to operations
		operations, err := core.GetFileOperationsWithContext([]types.Action{action}, ctx)
		require.NoError(t, err)

		// Verify we have all necessary operations
		require.Len(t, operations, 3, "Should have 3 operations: create dir, execute, write sentinel")

		// Check operation order and types
		assert.Equal(t, types.OperationCreateDir, operations[0].Type, "First operation should create directory")
		assert.Equal(t, types.OperationExecute, operations[1].Type, "Second operation should execute script")
		assert.Equal(t, types.OperationWriteFile, operations[2].Type, "Third operation should write sentinel")

		// Verify execute operation details
		executeOp := operations[1]
		assert.Equal(t, "/bin/sh", executeOp.Command, "Should use /bin/sh to execute")
		assert.Equal(t, []string{"/path/to/install.sh"}, executeOp.Args, "Should pass script path as argument")
		assert.Equal(t, "/path/to", executeOp.WorkingDir, "Should set working directory to script directory")
	})
}
