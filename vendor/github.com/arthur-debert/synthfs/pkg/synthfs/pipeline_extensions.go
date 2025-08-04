package synthfs

import (
	"context"
)

// ExecutablePipeline extends Pipeline with execution capabilities
type ExecutablePipeline interface {
	Pipeline
	Execute(ctx context.Context, fs FileSystem) (*Result, error)
	ExecuteWith(ctx context.Context, fs FileSystem, executor *Executor) (*Result, error)
}

// executablePipeline wraps a Pipeline to add execution methods
type executablePipeline struct {
	Pipeline
}

// NewExecutablePipeline creates a pipeline with execution capabilities
func NewExecutablePipeline() ExecutablePipeline {
	return &executablePipeline{
		Pipeline: NewMemPipeline(),
	}
}

// Execute runs the pipeline with a default executor
func (ep *executablePipeline) Execute(ctx context.Context, fs FileSystem) (*Result, error) {
	executor := NewExecutor()
	result := executor.Run(ctx, ep, fs)

	// Enhanced error handling
	ops := ep.Operations()
	for i, opResult := range result.GetOperations() {
		if opRes, ok := opResult.(OperationResult); ok && opRes.Error != nil {
			var successfulOps []OperationID
			for j := 0; j < i; j++ {
				if prevRes, ok := result.GetOperations()[j].(OperationResult); ok && prevRes.Error == nil {
					successfulOps = append(successfulOps, prevRes.OperationID)
				}
			}

			if i < len(ops) {
				return result, &PipelineError{
					FailedOp:      ops[i],
					FailedIndex:   i + 1,
					TotalOps:      len(ops),
					Err:           opRes.Error,
					SuccessfulOps: successfulOps,
				}
			}
			return result, opRes.Error
		}
	}

	return result, nil
}

// ExecuteWith runs the pipeline with a custom executor
func (ep *executablePipeline) ExecuteWith(ctx context.Context, fs FileSystem, executor *Executor) (*Result, error) {
	result := executor.Run(ctx, ep, fs)

	// Same error handling as Execute
	ops := ep.Operations()
	for i, opResult := range result.GetOperations() {
		if opRes, ok := opResult.(OperationResult); ok && opRes.Error != nil {
			var successfulOps []OperationID
			for j := 0; j < i; j++ {
				if prevRes, ok := result.GetOperations()[j].(OperationResult); ok && prevRes.Error == nil {
					successfulOps = append(successfulOps, prevRes.OperationID)
				}
			}

			if i < len(ops) {
				return result, &PipelineError{
					FailedOp:      ops[i],
					FailedIndex:   i + 1,
					TotalOps:      len(ops),
					Err:           opRes.Error,
					SuccessfulOps: successfulOps,
				}
			}
			return result, opRes.Error
		}
	}

	return result, nil
}
