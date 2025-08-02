package synthfs

import (
	"context"

	"github.com/arthur-debert/synthfs/pkg/synthfs/execution"
)

// Pipeline defines an interface for managing a sequence of operations.
type Pipeline interface {
	// Add appends one or more operations to the pipeline.
	// It may return an error, for example, if an operation with a duplicate ID
	// is added.
	Add(ops ...Operation) error

	// Operations returns all operations currently in the pipeline.
	// After Resolve() is called, this returns operations in dependency-resolved order.
	Operations() []Operation

	// Resolve performs dependency resolution using topological sorting.
	// This must be called before execution to ensure operations are in correct order.
	// Returns error if circular dependencies are detected.
	Resolve() error

	// Validate checks if all operations in the pipeline are valid.
	// This includes validating individual operations and checking for dependency conflicts.
	Validate(ctx context.Context, fs FileSystem) error
}

// NewMemPipeline creates a new in-memory operation pipeline.
func NewMemPipeline() Pipeline {
	logger := DefaultLogger()
	return &pipelineAdapter{
		pipeline: execution.NewMemPipeline(NewLoggerAdapter(&logger)),
	}
}

// pipelineAdapter adapts execution.Pipeline to our Pipeline interface
type pipelineAdapter struct {
	pipeline execution.Pipeline
}

// Add appends operations to the pipeline.
func (pa *pipelineAdapter) Add(ops ...Operation) error {
	// Convert Operation to operationWrapper for execution package
	var opsInterface []interface{}
	for _, op := range ops {
		wrapper := &operationWrapper{op: op}
		opsInterface = append(opsInterface, wrapper)
	}
	return pa.pipeline.Add(opsInterface...)
}

// Operations returns all operations currently in the pipeline.
func (pa *pipelineAdapter) Operations() []Operation {
	// Convert from interface{} back to Operation
	opsInterface := pa.pipeline.Operations()
	var ops []Operation
	for _, opInterface := range opsInterface {
		if wrapper, ok := opInterface.(*operationWrapper); ok {
			ops = append(ops, wrapper.op)
		} else if op, ok := opInterface.(Operation); ok {
			ops = append(ops, op)
		}
	}
	return ops
}

// Resolve performs dependency resolution using topological sorting.
func (pa *pipelineAdapter) Resolve() error {
	return pa.pipeline.Resolve()
}

// Validate checks if all operations in the pipeline are valid.
func (pa *pipelineAdapter) Validate(ctx context.Context, fs FileSystem) error {
	// The execution package expects a context and filesystem interface
	// We need to handle error conversion for ValidationError
	err := pa.pipeline.Validate(ctx, fs)
	if err != nil {
		// Check if we can recover the operation from the error message
		// For now, return the error as-is
		return err
	}
	return nil
}
