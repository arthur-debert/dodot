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
	UseCombinedExecutor bool // true for deploy/install, false for init/fill
}

// ExecuteOperations creates the appropriate executor and executes operations
func ExecuteOperations(opts ExecuteOperationsOptions) ([]types.OperationResult, error) {
	if opts.DryRun || len(opts.Operations) == 0 {
		return nil, nil
	}

	var executor interface {
		ExecuteOperations([]types.Operation) ([]types.OperationResult, error)
	}

	if opts.UseCombinedExecutor {
		combinedExecutor := synthfs.NewCombinedExecutor(opts.DryRun)
		if opts.EnableHomeSymlinks {
			combinedExecutor.EnableHomeSymlinks(true)
		}
		executor = combinedExecutor
	} else {
		executor = synthfs.NewSynthfsExecutor(opts.DryRun)
	}

	opResults, err := executor.ExecuteOperations(opts.Operations)
	if err != nil {
		return nil, fmt.Errorf("failed to execute operations: %w", err)
	}

	return opResults, nil
}
