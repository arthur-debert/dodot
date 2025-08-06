package types

import (
	"errors"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewExecutionContext(t *testing.T) {
	ctx := NewExecutionContext("deploy", false)

	assert.Equal(t, "deploy", ctx.Command)
	assert.False(t, ctx.DryRun)
	assert.NotNil(t, ctx.PackResults)
	assert.Empty(t, ctx.PackResults)
	assert.False(t, ctx.StartTime.IsZero())
	assert.True(t, ctx.EndTime.IsZero())
	assert.Equal(t, 0, ctx.TotalOperations)
}

func TestExecutionContextAddPackResult(t *testing.T) {
	ctx := NewExecutionContext("deploy", true)

	pack := &Pack{Name: "vim", Path: "/dotfiles/vim"}
	packResult := NewPackExecutionResult(pack)

	// Add some operation results
	packResult.AddOperationResult(&OperationResult{
		Operation: &Operation{Type: OperationCreateSymlink},
		Status:    StatusReady,
	})
	packResult.AddOperationResult(&OperationResult{
		Operation: &Operation{Type: OperationWriteFile},
		Status:    StatusSkipped,
	})

	ctx.AddPackResult("vim", packResult)

	assert.Len(t, ctx.PackResults, 1)
	assert.Equal(t, 2, ctx.TotalOperations)
	assert.Equal(t, 1, ctx.CompletedOperations)
	assert.Equal(t, 1, ctx.SkippedOperations)
	assert.Equal(t, 0, ctx.FailedOperations)
}

func TestExecutionContextMultiplePacks(t *testing.T) {
	ctx := NewExecutionContext("deploy", false)

	// Add first pack
	pack1 := &Pack{Name: "vim", Path: "/dotfiles/vim"}
	packResult1 := NewPackExecutionResult(pack1)
	packResult1.AddOperationResult(&OperationResult{
		Operation: &Operation{Type: OperationCreateSymlink},
		Status:    StatusReady,
	})
	packResult1.AddOperationResult(&OperationResult{
		Operation: &Operation{Type: OperationCreateSymlink},
		Status:    StatusError,
		Error:     errors.New("permission denied"),
	})

	// Add second pack
	pack2 := &Pack{Name: "bash", Path: "/dotfiles/bash"}
	packResult2 := NewPackExecutionResult(pack2)
	packResult2.AddOperationResult(&OperationResult{
		Operation: &Operation{Type: OperationWriteFile},
		Status:    StatusReady,
	})
	packResult2.AddOperationResult(&OperationResult{
		Operation: &Operation{Type: OperationWriteFile},
		Status:    StatusSkipped,
	})

	ctx.AddPackResult("vim", packResult1)
	ctx.AddPackResult("bash", packResult2)

	assert.Len(t, ctx.PackResults, 2)
	assert.Equal(t, 4, ctx.TotalOperations)
	assert.Equal(t, 2, ctx.CompletedOperations)
	assert.Equal(t, 1, ctx.SkippedOperations)
	assert.Equal(t, 1, ctx.FailedOperations)
}

func TestExecutionContextComplete(t *testing.T) {
	ctx := NewExecutionContext("deploy", false)
	assert.True(t, ctx.EndTime.IsZero())

	time.Sleep(10 * time.Millisecond)
	ctx.Complete()

	assert.False(t, ctx.EndTime.IsZero())
	assert.True(t, ctx.EndTime.After(ctx.StartTime))
}

func TestPackExecutionResult(t *testing.T) {
	pack := &Pack{Name: "vim", Path: "/dotfiles/vim"}
	result := NewPackExecutionResult(pack)

	assert.Equal(t, pack, result.Pack)
	assert.Empty(t, result.Operations)
	assert.Equal(t, ExecutionStatusPending, result.Status)
	assert.False(t, result.StartTime.IsZero())
	assert.True(t, result.EndTime.IsZero())
}

func TestPackExecutionResultStatusAggregation(t *testing.T) {
	tests := []struct {
		name           string
		operations     []OperationStatus
		expectedStatus ExecutionStatus
	}{
		{
			name:           "all success",
			operations:     []OperationStatus{StatusReady, StatusReady, StatusReady},
			expectedStatus: ExecutionStatusSuccess,
		},
		{
			name:           "all errors",
			operations:     []OperationStatus{StatusError, StatusError, StatusConflict},
			expectedStatus: ExecutionStatusError,
		},
		{
			name:           "all skipped",
			operations:     []OperationStatus{StatusSkipped, StatusSkipped},
			expectedStatus: ExecutionStatusSkipped,
		},
		{
			name:           "mixed with errors",
			operations:     []OperationStatus{StatusReady, StatusError, StatusSkipped},
			expectedStatus: ExecutionStatusPartial,
		},
		{
			name:           "success and skipped",
			operations:     []OperationStatus{StatusReady, StatusSkipped, StatusReady},
			expectedStatus: ExecutionStatusSuccess,
		},
		{
			name:           "no operations",
			operations:     []OperationStatus{},
			expectedStatus: ExecutionStatusPending,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pack := &Pack{Name: "test", Path: "/test"}
			result := NewPackExecutionResult(pack)

			for _, status := range tt.operations {
				opResult := &OperationResult{
					Operation: &Operation{Type: OperationCreateSymlink},
					Status:    status,
				}
				if status == StatusError || status == StatusConflict {
					opResult.Error = errors.New("test error")
				}
				result.AddOperationResult(opResult)
			}

			assert.Equal(t, tt.expectedStatus, result.Status)
		})
	}
}

func TestPackExecutionResultGrouping(t *testing.T) {
	pack := &Pack{Name: "vim", Path: "/dotfiles/vim"}
	result := NewPackExecutionResult(pack)

	// Add operations with different PowerUps and GroupIDs
	result.AddOperationResult(&OperationResult{
		Operation: &Operation{
			Type:    OperationCreateSymlink,
			PowerUp: "symlink",
			GroupID: "vim-config",
		},
		Status: StatusReady,
	})
	result.AddOperationResult(&OperationResult{
		Operation: &Operation{
			Type:    OperationCreateSymlink,
			PowerUp: "symlink",
			GroupID: "vim-config",
		},
		Status: StatusReady,
	})
	result.AddOperationResult(&OperationResult{
		Operation: &Operation{
			Type:    OperationWriteFile,
			PowerUp: "profile",
			GroupID: "vim-profile",
		},
		Status: StatusReady,
	})
	result.AddOperationResult(&OperationResult{
		Operation: &Operation{
			Type:    OperationExecute,
			PowerUp: "install",
			// No GroupID
		},
		Status: StatusReady,
	})

	// Test grouping by PowerUp
	powerUpGroups := result.GroupOperationsByPowerUp()
	assert.Len(t, powerUpGroups, 3)
	assert.Len(t, powerUpGroups["symlink"], 2)
	assert.Len(t, powerUpGroups["profile"], 1)
	assert.Len(t, powerUpGroups["install"], 1)

	// Test grouping by GroupID
	groupIDGroups := result.GroupOperationsByGroupID()
	assert.Len(t, groupIDGroups, 3)
	assert.Len(t, groupIDGroups["vim-config"], 2)
	assert.Len(t, groupIDGroups["vim-profile"], 1)
	assert.Len(t, groupIDGroups["ungrouped"], 1)
}

func TestOperationResultWithContext(t *testing.T) {
	op := &Operation{
		Type:        OperationCreateSymlink,
		Source:      "/source/file",
		Target:      "/target/file",
		Description: "Link config file",
		Pack:        "vim",
		PowerUp:     "symlink",
		TriggerInfo: &TriggerMatchInfo{
			TriggerName:  "FileName",
			OriginalPath: ".vimrc",
			Priority:     10,
		},
		Metadata: map[string]interface{}{
			"app": "vim",
		},
	}

	startTime := time.Now()
	result := &OperationResult{
		Operation: op,
		Status:    StatusReady,
		StartTime: startTime,
		EndTime:   startTime.Add(100 * time.Millisecond),
		Metadata: map[string]interface{}{
			"executionNote": "symlink created successfully",
		},
	}

	assert.Equal(t, op, result.Operation)
	assert.Equal(t, StatusReady, result.Status)
	assert.Nil(t, result.Error)
	assert.Equal(t, "vim", result.Operation.Pack)
	assert.Equal(t, "symlink", result.Operation.PowerUp)
	assert.Equal(t, "FileName", result.Operation.TriggerInfo.TriggerName)
	assert.Equal(t, "symlink created successfully", result.Metadata["executionNote"])
}

func TestExecutionContextGetPackResult(t *testing.T) {
	ctx := NewExecutionContext("deploy", false)

	pack := &Pack{Name: "vim", Path: "/dotfiles/vim"}
	packResult := NewPackExecutionResult(pack)
	ctx.AddPackResult("vim", packResult)

	// Test existing pack
	result, ok := ctx.GetPackResult("vim")
	require.True(t, ok)
	assert.Equal(t, packResult, result)

	// Test non-existing pack
	result, ok = ctx.GetPackResult("bash")
	assert.False(t, ok)
	assert.Nil(t, result)
}

func TestPackExecutionResultComplete(t *testing.T) {
	pack := &Pack{Name: "vim", Path: "/dotfiles/vim"}
	result := NewPackExecutionResult(pack)

	assert.True(t, result.EndTime.IsZero())

	// Add some operations
	result.AddOperationResult(&OperationResult{
		Operation: &Operation{Type: OperationCreateSymlink},
		Status:    StatusReady,
	})

	time.Sleep(10 * time.Millisecond)
	result.Complete()

	assert.False(t, result.EndTime.IsZero())
	assert.True(t, result.EndTime.After(result.StartTime))
	assert.Equal(t, ExecutionStatusSuccess, result.Status)
}
