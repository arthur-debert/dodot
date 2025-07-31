package synthfs

import (
	"context"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
	"github.com/arthur-debert/synthfs/pkg/synthfs/execution"
)

// PipelineOptions controls how operations are executed
type PipelineOptions = core.PipelineOptions

// OperationResult holds the outcome of a single operation's execution
type OperationResult struct {
	OperationID  OperationID
	Operation    Operation // The operation that was executed
	Status       OperationStatus
	Error        error
	Duration     time.Duration
	BackupData   *BackupData
	BackupSizeMB float64
}

// DefaultPipelineOptions returns sensible defaults for pipeline execution
func DefaultPipelineOptions() PipelineOptions {
	return execution.DefaultPipelineOptions()
}

// Executor processes a pipeline of operations.
type Executor struct {
	executor *execution.Executor
}

// NewExecutor creates a new Executor.
func NewExecutor() *Executor {
	logger := DefaultLogger()
	return &Executor{
		executor: execution.NewExecutor(NewLoggerAdapter(&logger)),
	}
}

// EventBus returns the executor's event bus for subscription
func (e *Executor) EventBus() core.EventBus {
	return e.executor.EventBus()
}

// Run runs all operations in the pipeline with default options.
func (e *Executor) Run(ctx context.Context, pipeline Pipeline, fs FileSystem) *Result {
	return e.RunWithOptions(ctx, pipeline, fs, DefaultPipelineOptions())
}

// RunWithOptions runs all operations in the pipeline with specified options.
func (e *Executor) RunWithOptions(ctx context.Context, pipeline Pipeline, fs FileSystem, opts PipelineOptions) *Result {
	// Create pipeline wrapper
	wrapper := &pipelineWrapper{pipeline: pipeline}
	coreResult := e.executor.RunWithOptions(ctx, wrapper, fs, opts)

	// Convert core.Result back to main package Result
	return e.convertResult(coreResult)
}

// convertResult converts core.Result to main package Result
func (e *Executor) convertResult(coreResult *core.Result) *Result {
	// Create operations list
	operations := make([]interface{}, 0, len(coreResult.Operations))
	for _, coreOpResult := range coreResult.Operations {
		opResult := OperationResult{
			OperationID:  coreOpResult.OperationID,
			Status:       coreOpResult.Status,
			Error:        coreOpResult.Error,
			Duration:     coreOpResult.Duration,
			BackupData:   coreOpResult.BackupData,
			BackupSizeMB: coreOpResult.BackupSizeMB,
		}

		// Convert operation from interface{} back to Operation
		if op, ok := coreOpResult.Operation.(Operation); ok {
			opResult.Operation = op
		} else if wrapper, ok := coreOpResult.Operation.(*operationWrapper); ok {
			opResult.Operation = wrapper.op
		}

		operations = append(operations, opResult)
	}

	// Convert restore operations
	restoreOps := make([]interface{}, 0, len(coreResult.RestoreOps))
	restoreOps = append(restoreOps, coreResult.RestoreOps...)

	// Get first error if any
	var firstErr error
	if len(coreResult.Errors) > 0 {
		firstErr = coreResult.Errors[0]
	}

	// Create the result using the simplified structure
	return &Result{
		success:    coreResult.Success,
		operations: operations,
		restoreOps: restoreOps,
		duration:   coreResult.Duration,
		err:        firstErr,
		budget:     coreResult.Budget,
		rollback:   coreResult.Rollback,
	}
}

// pipelineWrapper wraps Pipeline to implement execution.PipelineInterface
type pipelineWrapper struct {
	pipeline Pipeline
}

func (pw *pipelineWrapper) Add(ops ...interface{}) error {
	// Convert operations and add to pipeline
	for _, op := range ops {
		if opTyped, ok := op.(Operation); ok {
			if err := pw.pipeline.Add(opTyped); err != nil {
				return err
			}
		}
	}
	return nil
}

func (pw *pipelineWrapper) Operations() []interface{} {
	ops := pw.pipeline.Operations()
	var result []interface{}
	for _, op := range ops {
		result = append(result, &operationWrapper{op: op})
	}
	return result
}

func (pw *pipelineWrapper) Resolve() error {
	return pw.pipeline.Resolve()
}

func (pw *pipelineWrapper) Validate(ctx context.Context, fs interface{}) error {
	fsys, ok := fs.(FileSystem)
	if !ok {
		return nil // Skip validation if filesystem interface doesn't match
	}
	return pw.pipeline.Validate(ctx, fsys)
}

func (pw *pipelineWrapper) ResolvePrerequisites(resolver core.PrerequisiteResolver, fs interface{}) error {
	// The main package pipeline doesn't support prerequisite resolution yet
	// This is a no-op for now
	return nil
}

// operationWrapper wraps Operation to implement execution.OperationInterface
type operationWrapper struct {
	op Operation
}

func (ow *operationWrapper) ID() core.OperationID {
	return ow.op.ID()
}

func (ow *operationWrapper) Describe() core.OperationDesc {
	return ow.op.Describe()
}

func (ow *operationWrapper) Dependencies() []core.OperationID {
	return ow.op.Dependencies()
}

func (ow *operationWrapper) Conflicts() []core.OperationID {
	return ow.op.Conflicts()
}

func (ow *operationWrapper) Prerequisites() []core.Prerequisite {
	// Check if operation implements Prerequisites method
	if prereqOp, ok := ow.op.(interface{ Prerequisites() []core.Prerequisite }); ok {
		return prereqOp.Prerequisites()
	}
	return []core.Prerequisite{}
}

func (ow *operationWrapper) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	// Delegate to the original operation's ExecuteV2 if available, otherwise Execute
	if execV2Op, ok := ow.op.(interface {
		ExecuteV2(interface{}, *core.ExecutionContext, interface{}) error
	}); ok {
		return execV2Op.ExecuteV2(ctx, execCtx, fsys)
	}

	// Fallback to original Execute method
	if contextOp, ok := ctx.(context.Context); ok {
		if fsysOp, ok := fsys.(FileSystem); ok {
			return ow.op.Execute(contextOp, fsysOp)
		}
	}
	return nil
}

func (ow *operationWrapper) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	// Delegate to the original operation's ValidateV2 if available, otherwise Validate
	if validateV2Op, ok := ow.op.(interface {
		ValidateV2(interface{}, *core.ExecutionContext, interface{}) error
	}); ok {
		return validateV2Op.ValidateV2(ctx, execCtx, fsys)
	}

	// Fallback to original Validate method
	if contextOp, ok := ctx.(context.Context); ok {
		if fsysOp, ok := fsys.(FileSystem); ok {
			return ow.op.Validate(contextOp, fsysOp)
		}
	}
	return nil
}

func (ow *operationWrapper) ReverseOps(ctx context.Context, fsys interface{}, budget *core.BackupBudget) ([]interface{}, *core.BackupData, error) {
	// Delegate to the original operation's ReverseOps
	if fsysOp, ok := fsys.(FileSystem); ok {
		reverseOps, backupData, err := ow.op.ReverseOps(ctx, fsysOp, budget)

		// Convert []Operation to []interface{} even if there's an error
		// This preserves partial backup data in case of budget exhaustion
		var result []interface{}
		for _, op := range reverseOps {
			result = append(result, op)
		}
		return result, backupData, err
	}
	return nil, nil, nil
}

func (ow *operationWrapper) Rollback(ctx context.Context, fsys interface{}) error {
	// Delegate to the original operation's Rollback
	if fsysOp, ok := fsys.(FileSystem); ok {
		return ow.op.Rollback(ctx, fsysOp)
	}
	return nil
}

func (ow *operationWrapper) GetItem() interface{} {
	return ow.op.GetItem()
}

func (ow *operationWrapper) GetSrcPath() string {
	if adapter, ok := ow.op.(*OperationsPackageAdapter); ok {
		src, _ := adapter.opsOperation.GetPaths()
		return src
	}
	return ""
}

func (ow *operationWrapper) GetDstPath() string {
	if adapter, ok := ow.op.(*OperationsPackageAdapter); ok {
		_, dst := adapter.opsOperation.GetPaths()
		return dst
	}
	return ""
}

func (ow *operationWrapper) AddDependency(depID core.OperationID) {
	// Delegate to the original operation's AddDependency method
	ow.op.AddDependency(depID)
}

func (ow *operationWrapper) SetDescriptionDetail(key string, value interface{}) {
	// Delegate to the original operation's SetDescriptionDetail method
	ow.op.SetDescriptionDetail(key, value)
}
