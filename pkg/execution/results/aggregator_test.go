package results_test

import (
	"errors"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/execution"
	"github.com/arthur-debert/dodot/pkg/execution/context"
	"github.com/arthur-debert/dodot/pkg/execution/results"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestAggregator_CreatePackResult(t *testing.T) {
	a := results.NewAggregator()
	pack := &types.Pack{
		Name: "vim",
		Path: "/dotfiles/vim",
	}

	result := a.CreatePackResult(pack)

	assert.Equal(t, pack, result.Pack)
	assert.Equal(t, execution.ExecutionStatusPending, result.Status)
	assert.NotZero(t, result.StartTime)
	assert.Zero(t, result.EndTime)
	assert.Empty(t, result.HandlerResults)
	assert.Equal(t, 0, result.TotalHandlers)
}

func TestAggregator_AddHandlerResult(t *testing.T) {
	tests := []struct {
		name              string
		handlerResults    []*context.HandlerResult
		expectedTotal     int
		expectedCompleted int
		expectedFailed    int
		expectedSkipped   int
		expectedStatus    execution.ExecutionStatus
	}{
		{
			name: "all handlers succeed",
			handlerResults: []*context.HandlerResult{
				{HandlerName: "symlink", Status: execution.StatusReady},
				{HandlerName: "shell", Status: execution.StatusReady},
			},
			expectedTotal:     2,
			expectedCompleted: 2,
			expectedFailed:    0,
			expectedSkipped:   0,
			expectedStatus:    execution.ExecutionStatusSuccess,
		},
		{
			name: "all handlers fail",
			handlerResults: []*context.HandlerResult{
				{HandlerName: "symlink", Status: execution.StatusError},
				{HandlerName: "shell", Status: execution.StatusConflict},
			},
			expectedTotal:     2,
			expectedCompleted: 0,
			expectedFailed:    2,
			expectedSkipped:   0,
			expectedStatus:    execution.ExecutionStatusError,
		},
		{
			name: "all handlers skipped",
			handlerResults: []*context.HandlerResult{
				{HandlerName: "symlink", Status: execution.StatusSkipped},
				{HandlerName: "shell", Status: execution.StatusSkipped},
			},
			expectedTotal:     2,
			expectedCompleted: 0,
			expectedFailed:    0,
			expectedSkipped:   2,
			expectedStatus:    execution.ExecutionStatusSkipped,
		},
		{
			name: "mixed results - partial success",
			handlerResults: []*context.HandlerResult{
				{HandlerName: "symlink", Status: execution.StatusReady},
				{HandlerName: "shell", Status: execution.StatusError},
				{HandlerName: "path", Status: execution.StatusSkipped},
			},
			expectedTotal:     3,
			expectedCompleted: 1,
			expectedFailed:    1,
			expectedSkipped:   1,
			expectedStatus:    execution.ExecutionStatusPartial,
		},
		{
			name:              "no handlers",
			handlerResults:    []*context.HandlerResult{},
			expectedTotal:     0,
			expectedCompleted: 0,
			expectedFailed:    0,
			expectedSkipped:   0,
			expectedStatus:    execution.ExecutionStatusPending,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			a := results.NewAggregator()
			pack := &types.Pack{Name: "test-pack"}
			result := a.CreatePackResult(pack)

			// Add all handler results
			for _, hr := range tt.handlerResults {
				a.AddHandlerResult(result, hr)
			}

			assert.Equal(t, tt.expectedTotal, result.TotalHandlers)
			assert.Equal(t, tt.expectedCompleted, result.CompletedHandlers)
			assert.Equal(t, tt.expectedFailed, result.FailedHandlers)
			assert.Equal(t, tt.expectedSkipped, result.SkippedHandlers)
			assert.Equal(t, tt.expectedStatus, result.Status)
			assert.Len(t, result.HandlerResults, len(tt.handlerResults))
		})
	}
}

func TestAggregator_CompletePackResult(t *testing.T) {
	a := results.NewAggregator()
	pack := &types.Pack{Name: "vim"}
	result := a.CreatePackResult(pack)

	// Add a handler result
	a.AddHandlerResult(result, &context.HandlerResult{
		HandlerName: "symlink",
		Status:      execution.StatusReady,
	})

	// Initially EndTime should be zero
	assert.Zero(t, result.EndTime)

	// Sleep briefly to ensure EndTime > StartTime
	time.Sleep(10 * time.Millisecond)

	a.CompletePackResult(result)

	assert.NotZero(t, result.EndTime)
	assert.True(t, result.EndTime.After(result.StartTime))
	assert.Equal(t, execution.ExecutionStatusSuccess, result.Status)
}

func TestAggregator_StatusCalculation(t *testing.T) {
	tests := []struct {
		name           string
		setupFunc      func(*context.PackExecutionResult)
		expectedStatus execution.ExecutionStatus
	}{
		{
			name: "conflict status counts as failure",
			setupFunc: func(per *context.PackExecutionResult) {
				per.HandlerResults = []*context.HandlerResult{
					{Status: execution.StatusReady},
					{Status: execution.StatusConflict},
				}
				per.TotalHandlers = 2
				per.CompletedHandlers = 1
				per.FailedHandlers = 1
			},
			expectedStatus: execution.ExecutionStatusPartial,
		},
		{
			name: "error with details",
			setupFunc: func(per *context.PackExecutionResult) {
				per.HandlerResults = []*context.HandlerResult{
					{
						Status: execution.StatusError,
						Error:  errors.New("permission denied"),
					},
				}
				per.TotalHandlers = 1
				per.FailedHandlers = 1
			},
			expectedStatus: execution.ExecutionStatusError,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			a := results.NewAggregator()
			pack := &types.Pack{Name: "test"}
			result := a.CreatePackResult(pack)

			// Reset the initial state before applying test setup
			result.TotalHandlers = 0
			result.CompletedHandlers = 0
			result.FailedHandlers = 0
			result.SkippedHandlers = 0
			result.HandlerResults = nil

			tt.setupFunc(result)

			// Add a dummy handler result to trigger status update
			a.CompletePackResult(result)

			assert.Equal(t, tt.expectedStatus, result.Status)
		})
	}
}
