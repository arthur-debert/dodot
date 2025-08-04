package synthfs

import (
	"context"

	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// PipelineBuilder provides a fluent API for building and executing pipelines
type PipelineBuilder struct {
	pipeline     Pipeline
	dependencies map[OperationID][]OperationID
	lastOp       Operation
}

// BuildPipeline creates a new pipeline with the given operations
func BuildPipeline(ops ...Operation) *PipelineBuilder {
	pb := &PipelineBuilder{
		pipeline:     NewMemPipeline(),
		dependencies: make(map[OperationID][]OperationID),
	}

	// Track operations that create paths
	pathCreators := make(map[string]Operation)

	for _, op := range ops {
		// Auto-detect dependencies based on paths
		if adapter, ok := op.(*OperationsPackageAdapter); ok {
			srcPath, dstPath := adapter.opsOperation.GetPaths()
			desc := adapter.opsOperation.Describe()
			opType := desc.Type

			// For operations that read from a source, check if source was created by a previous op
			if srcPath != "" && (opType == "copy" || opType == "move") {
				if creator, exists := pathCreators[srcPath]; exists {
					op.AddDependency(creator.ID())
				}
			}

			// Track paths this operation creates
			if dstPath != "" {
				pathCreators[dstPath] = op
			} else if srcPath != "" && (opType == "create_file" ||
				opType == "create_directory" ||
				opType == "create_symlink") {
				pathCreators[srcPath] = op
			}
		}

		if err := pb.pipeline.Add(op); err == nil {
			pb.lastOp = op
		}
	}

	return pb
}

// NewPipelineBuilder creates a new empty pipeline builder
func NewPipelineBuilder() *PipelineBuilder {
	return &PipelineBuilder{
		pipeline:     NewMemPipeline(),
		dependencies: make(map[OperationID][]OperationID),
	}
}

// Add adds an operation to the pipeline
func (pb *PipelineBuilder) Add(op Operation) *PipelineBuilder {
	if err := pb.pipeline.Add(op); err == nil {
		pb.lastOp = op
	}
	return pb
}

// After specifies that the last added operation depends on the given operations
func (pb *PipelineBuilder) After(deps ...Operation) *PipelineBuilder {
	if pb.lastOp != nil {
		for _, dep := range deps {
			pb.lastOp.AddDependency(dep.ID())
		}
	}
	return pb
}

// WithDependency adds a dependency between two operations
func (pb *PipelineBuilder) WithDependency(dependent, dependency Operation) *PipelineBuilder {
	dependent.AddDependency(dependency.ID())
	return pb
}

// Build returns the built pipeline
func (pb *PipelineBuilder) Build() Pipeline {
	return pb.pipeline
}

// Execute runs the pipeline against the given filesystem.
func (pb *PipelineBuilder) Execute(ctx context.Context, fs filesystem.FullFileSystem) (*Result, error) {
	// For BuildPipeline, use sequential execution like simple_api to handle dependencies
	// The pipeline/executor approach validates all operations upfront which fails for
	// operations that depend on files created by previous operations
	ops := pb.pipeline.Operations()
	return RunWithOptions(ctx, fs, DefaultPipelineOptions(), ops...)
}

// ExecuteWith runs the pipeline with a custom executor
func (pb *PipelineBuilder) ExecuteWith(ctx context.Context, fs FileSystem, executor *Executor) (*Result, error) {
	result := executor.Run(ctx, pb.pipeline, fs)

	// Check for errors (same as Execute)
	for i, opResult := range result.GetOperations() {
		if opRes, ok := opResult.(OperationResult); ok && opRes.Error != nil {
			var successfulOps []OperationID
			for j := 0; j < i; j++ {
				if prevRes, ok := result.GetOperations()[j].(OperationResult); ok && prevRes.Error == nil {
					successfulOps = append(successfulOps, prevRes.OperationID)
				}
			}

			ops := pb.pipeline.Operations()
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

// WithOptions sets pipeline options and executes
func (pb *PipelineBuilder) WithOptions(options PipelineOptions) *PipelineExecutor {
	return &PipelineExecutor{
		pipeline: pb.pipeline,
		options:  options,
	}
}

// PipelineExecutor handles execution with options
type PipelineExecutor struct {
	pipeline Pipeline
	options  PipelineOptions
}

// Execute runs the pipeline with the configured options
func (pe *PipelineExecutor) Execute(ctx context.Context, fs FileSystem) (*Result, error) {
	executor := NewExecutor()
	result := executor.RunWithOptions(ctx, pe.pipeline, fs, pe.options)

	// Check for errors
	for i, opResult := range result.GetOperations() {
		if opRes, ok := opResult.(OperationResult); ok && opRes.Error != nil {
			var successfulOps []OperationID
			for j := 0; j < i; j++ {
				if prevRes, ok := result.GetOperations()[j].(OperationResult); ok && prevRes.Error == nil {
					successfulOps = append(successfulOps, prevRes.OperationID)
				}
			}

			ops := pe.pipeline.Operations()
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
