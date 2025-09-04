package results

import (
	"time"

	"github.com/arthur-debert/dodot/pkg/execution"
	"github.com/arthur-debert/dodot/pkg/types"
)

// Aggregator handles the business logic for PackExecutionResult operations.
// It provides methods to manipulate pack results without embedding
// business logic in the data structure itself.
type Aggregator struct{}

// NewAggregator creates a new results aggregator
func NewAggregator() *Aggregator {
	return &Aggregator{}
}

// CreatePackResult creates a new pack execution result
func (a *Aggregator) CreatePackResult(pack *types.Pack) *types.PackExecutionResult {
	return &types.PackExecutionResult{
		Pack:           pack,
		HandlerResults: make([]*types.HandlerResult, 0),
		Status:         execution.ExecutionStatusPending,
		StartTime:      time.Now(),
	}
}

// AddHandlerResult adds a handler result and updates statistics
func (a *Aggregator) AddHandlerResult(per *types.PackExecutionResult, result *types.HandlerResult) {
	per.HandlerResults = append(per.HandlerResults, result)
	per.TotalHandlers++

	switch result.Status {
	case execution.StatusReady:
		per.CompletedHandlers++
	case execution.StatusSkipped:
		per.SkippedHandlers++
	case execution.StatusError, execution.StatusConflict:
		per.FailedHandlers++
	}

	// Update pack status
	a.updateStatus(per)
}

// CompletePackResult marks the pack execution as complete
func (a *Aggregator) CompletePackResult(per *types.PackExecutionResult) {
	per.EndTime = time.Now()
	a.updateStatus(per)
}

// updateStatus recalculates the pack's aggregated status based on handler results
func (a *Aggregator) updateStatus(per *types.PackExecutionResult) {
	if per.TotalHandlers == 0 {
		per.Status = execution.ExecutionStatusPending
		return
	}

	if per.FailedHandlers == per.TotalHandlers {
		per.Status = execution.ExecutionStatusError
	} else if per.SkippedHandlers == per.TotalHandlers {
		per.Status = execution.ExecutionStatusSkipped
	} else if per.FailedHandlers > 0 {
		per.Status = execution.ExecutionStatusPartial
	} else {
		per.Status = execution.ExecutionStatusSuccess
	}
}
