package context

import (
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
)

// Manager handles the business logic for ExecutionContext operations.
// It provides methods to manipulate ExecutionContext without embedding
// business logic in the data structure itself.
type Manager struct{}

// NewManager creates a new execution context manager
func NewManager() *Manager {
	return &Manager{}
}

// CreateContext creates a new execution context
func (m *Manager) CreateContext(command string, dryRun bool) *types.ExecutionContext {
	return &types.ExecutionContext{
		Command:     command,
		PackResults: make(map[string]*types.PackExecutionResult),
		StartTime:   time.Now(),
		DryRun:      dryRun,
	}
}

// AddPackResult adds or updates a pack result and recalculates totals
func (m *Manager) AddPackResult(ec *types.ExecutionContext, packName string, result *types.PackExecutionResult) {
	ec.PackResults[packName] = result
	m.recalculateTotals(ec)
}

// CompleteContext marks the execution as complete
func (m *Manager) CompleteContext(ec *types.ExecutionContext) {
	ec.EndTime = time.Now()
}

// recalculateTotals updates the aggregated handler counts across all packs
func (m *Manager) recalculateTotals(ec *types.ExecutionContext) {
	ec.TotalHandlers = 0
	ec.CompletedHandlers = 0
	ec.FailedHandlers = 0
	ec.SkippedHandlers = 0

	for _, pr := range ec.PackResults {
		ec.TotalHandlers += pr.TotalHandlers
		ec.CompletedHandlers += pr.CompletedHandlers
		ec.FailedHandlers += pr.FailedHandlers
		ec.SkippedHandlers += pr.SkippedHandlers
	}
}
