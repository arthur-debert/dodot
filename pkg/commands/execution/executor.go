package execution

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/synthfs"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ExecuteOperationsOptions contains options for executing operations
type ExecuteOperationsOptions struct {
	Operations          []types.Operation
	DryRun              bool
	EnableHomeSymlinks  bool
	UseCombinedExecutor bool // Deprecated: all operations now use SynthfsExecutor
}

// ExecuteOperations creates the appropriate executor and executes operations
func ExecuteOperations(opts ExecuteOperationsOptions) ([]types.OperationResult, error) {
	if opts.DryRun || len(opts.Operations) == 0 {
		return nil, nil
	}

	// Always use SynthfsExecutor now that it supports shell commands via synthfs
	executor := synthfs.NewSynthfsExecutor(opts.DryRun)
	if opts.EnableHomeSymlinks {
		executor.EnableHomeSymlinks(true)
	}

	opResults, err := executor.ExecuteOperations(opts.Operations)
	if err != nil {
		return nil, fmt.Errorf("failed to execute operations: %w", err)
	}

	return opResults, nil
}

// ExecuteActionsDirectly provides a bridge for direct execution compatibility.
// Since DirectExecutor executes during pipeline execution, this function is primarily
// for interface compatibility and returns operation results from ExecutionContext.
func ExecuteActionsDirectly(context *types.ExecutionContext) ([]types.OperationResult, error) {
	if context == nil {
		return nil, nil
	}

	var allResults []types.OperationResult

	// Extract operation results from all pack execution results
	for _, packResult := range context.PackResults {
		for _, opResult := range packResult.Operations {
			allResults = append(allResults, *opResult)
		}
	}

	return allResults, nil
}
