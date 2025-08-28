// pkg/types/execution_context_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: None
// PURPOSE: Test ExecutionContext type methods

package types_test

import (
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestNewExecutionContext(t *testing.T) {
	tests := []struct {
		name    string
		command string
		dryRun  bool
	}{
		{
			name:    "deploy_command_dry_run",
			command: "link",
			dryRun:  true,
		},
		{
			name:    "install_command_real_run",
			command: "provision",
			dryRun:  false,
		},
		{
			name:    "status_command",
			command: "status",
			dryRun:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ec := types.NewExecutionContext(tt.command, tt.dryRun)

			assert.Equal(t, tt.command, ec.Command)
			assert.Equal(t, tt.dryRun, ec.DryRun)
			assert.NotNil(t, ec.PackResults)
			assert.Empty(t, ec.PackResults)
			assert.False(t, ec.StartTime.IsZero())
			assert.True(t, ec.EndTime.IsZero())
			assert.Equal(t, 0, ec.TotalActions)
			assert.Equal(t, 0, ec.CompletedActions)
			assert.Equal(t, 0, ec.FailedActions)
			assert.Equal(t, 0, ec.SkippedActions)
		})
	}
}

func TestExecutionContext_AddPackResult(t *testing.T) {
	ec := types.NewExecutionContext("link", false)

	// Create pack results with different statuses
	pack1Result := &types.PackExecutionResult{
		Pack:              &types.Pack{Name: "vim"},
		TotalHandlers:     5,
		CompletedHandlers: 3,
		FailedHandlers:    1,
		SkippedHandlers:   1,
	}

	pack2Result := &types.PackExecutionResult{
		Pack:              &types.Pack{Name: "zsh"},
		TotalHandlers:     3,
		CompletedHandlers: 2,
		FailedHandlers:    0,
		SkippedHandlers:   1,
	}

	// Add first pack
	ec.AddPackResult("vim", pack1Result)
	assert.Equal(t, 1, len(ec.PackResults))
	assert.Equal(t, 5, ec.TotalActions)
	assert.Equal(t, 3, ec.CompletedActions)
	assert.Equal(t, 1, ec.FailedActions)
	assert.Equal(t, 1, ec.SkippedActions)

	// Add second pack
	ec.AddPackResult("zsh", pack2Result)
	assert.Equal(t, 2, len(ec.PackResults))
	assert.Equal(t, 8, ec.TotalActions)
	assert.Equal(t, 5, ec.CompletedActions)
	assert.Equal(t, 1, ec.FailedActions)
	assert.Equal(t, 2, ec.SkippedActions)
}

func TestExecutionContext_Complete(t *testing.T) {
	ec := types.NewExecutionContext("status", false)

	// Initially end time should be zero
	assert.True(t, ec.EndTime.IsZero())

	// Complete the context
	ec.Complete()

	// End time should be set
	assert.False(t, ec.EndTime.IsZero())
	assert.True(t, ec.EndTime.After(ec.StartTime))
}

func TestHandlerResult_Structure(t *testing.T) {
	hr := &types.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{".vimrc", ".gvimrc"},
		Status:      types.StatusReady,
		Error:       nil,
		StartTime:   time.Now(),
		EndTime:     time.Now().Add(100 * time.Millisecond),
	}

	assert.Equal(t, "symlink", hr.HandlerName)
	assert.Len(t, hr.Files, 2)
	assert.Contains(t, hr.Files, ".vimrc")
	assert.Contains(t, hr.Files, ".gvimrc")
	assert.Equal(t, types.StatusReady, hr.Status)
	assert.Nil(t, hr.Error)
	assert.True(t, hr.EndTime.After(hr.StartTime))
}
