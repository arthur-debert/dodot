package execution

import (
	"context"
	"fmt"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// Executor processes a pipeline of operations
type Executor struct {
	logger   core.Logger
	eventBus core.EventBus
}

// NewExecutor creates a new Executor
func NewExecutor(logger core.Logger) *Executor {
	return &Executor{
		logger:   logger,
		eventBus: core.NewMemoryEventBus(logger),
	}
}

// EventBus returns the executor's event bus for subscription
func (e *Executor) EventBus() core.EventBus {
	return e.eventBus
}

// DefaultPipelineOptions returns sensible defaults for pipeline execution
func DefaultPipelineOptions() core.PipelineOptions {
	return core.PipelineOptions{
		Restorable:           false,                   // No backup overhead by default
		MaxBackupSizeMB:      core.DefaultMaxBackupMB, // Default budget
		ResolvePrerequisites: true,                    // Enable prerequisite resolution by default (Phase 6)
		UseSimpleBatch:       true,                    // Use SimpleBatch by default (Phase 6)
	}
}

// OperationInterface defines the minimal interface needed by executor
type OperationInterface interface {
	ID() core.OperationID
	Describe() core.OperationDesc
	Dependencies() []core.OperationID
	Conflicts() []core.OperationID
	Prerequisites() []core.Prerequisite
	AddDependency(depID core.OperationID)
	ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error
	ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error
	ReverseOps(ctx context.Context, fsys interface{}, budget *core.BackupBudget) ([]interface{}, *core.BackupData, error)
	Rollback(ctx context.Context, fsys interface{}) error
	GetItem() interface{}
	SetDescriptionDetail(key string, value interface{})
}

// PipelineInterface defines the minimal interface needed by executor
type PipelineInterface interface {
	Add(ops ...interface{}) error
	Operations() []interface{}
	Resolve() error
	ResolvePrerequisites(resolver core.PrerequisiteResolver, fs interface{}) error
	Validate(ctx context.Context, fs interface{}) error
}

// Run runs all operations in the pipeline with default options
func (e *Executor) Run(ctx context.Context, pipeline PipelineInterface, fs interface{}) *core.Result {
	return e.RunWithOptions(ctx, pipeline, fs, DefaultPipelineOptions())
}

// RunWithOptions runs all operations in the pipeline with specified options
func (e *Executor) RunWithOptions(ctx context.Context, pipeline PipelineInterface, fs interface{}, opts core.PipelineOptions) *core.Result {
	return e.RunWithOptionsAndResolver(ctx, pipeline, fs, opts, nil)
}

// RunWithOptionsAndResolver runs all operations in the pipeline with specified options and a custom prerequisite resolver
func (e *Executor) RunWithOptionsAndResolver(ctx context.Context, pipeline interface{}, fs interface{}, opts core.PipelineOptions, resolver core.PrerequisiteResolver) *core.Result {
	// Convert pipeline to PipelineInterface
	var pipelineInterface PipelineInterface
	if p, ok := pipeline.(PipelineInterface); ok {
		pipelineInterface = p
	} else if p, ok := pipeline.(Pipeline); ok {
		// Adapt Pipeline to PipelineInterface
		pipelineInterface = &pipelineAdapter{pipeline: p}
	} else {
		result := &core.Result{
			Success: false,
			Errors:  []error{fmt.Errorf("invalid pipeline type: %T", pipeline)},
		}
		return result
	}

	e.logger.Info().
		Int("operation_count", len(pipelineInterface.Operations())).
		Bool("restorable", opts.Restorable).
		Int("max_backup_mb", opts.MaxBackupSizeMB).
		Bool("resolve_prerequisites", opts.ResolvePrerequisites).
		Msg("starting execution")

	start := time.Now()
	result := &core.Result{
		Operations: []core.OperationResult{},
		Errors:     []error{},
		Success:    true,
		RestoreOps: []interface{}{},
	}

	// Initialize budget if restorable mode is enabled
	var budget *core.BackupBudget
	if opts.Restorable {
		budget = &core.BackupBudget{
			TotalMB:     float64(opts.MaxBackupSizeMB),
			RemainingMB: float64(opts.MaxBackupSizeMB),
			UsedMB:      0,
		}
		result.Budget = budget

		e.logger.Info().
			Float64("total_budget_mb", budget.TotalMB).
			Msg("backup budget initialized for restorable execution")
	}

	// Create execution context
	execCtx := &core.ExecutionContext{
		Logger:   e.logger,
		Budget:   budget,
		EventBus: e.eventBus,
	}

	// Resolve prerequisites if enabled
	if opts.ResolvePrerequisites {
		e.logger.Info().Msg("resolving operation prerequisites")

		// Use provided resolver or create default one
		prereqResolver := resolver
		if prereqResolver == nil {
			e.logger.Debug().Msg("no prerequisite resolver provided and no factory available - skipping prerequisite resolution")
			// Cannot create a default resolver without a factory, so skip prerequisite resolution
			e.logger.Info().Msg("prerequisite resolution skipped - no resolver provided")
		} else {
			if err := pipelineInterface.ResolvePrerequisites(prereqResolver, fs); err != nil {
				e.logger.Info().Err(err).Msg("prerequisite resolution failed")
				result.Success = false
				result.Errors = append(result.Errors, fmt.Errorf("prerequisite resolution failed: %w", err))
				result.Duration = time.Since(start)
				return result
			}
			e.logger.Info().Msg("prerequisite resolution completed successfully")
		}
	}

	// Resolve dependencies
	e.logger.Info().Msg("resolving operation dependencies")
	if err := pipelineInterface.Resolve(); err != nil {
		e.logger.Info().Err(err).Msg("dependency resolution failed")
		result.Success = false
		result.Errors = append(result.Errors, fmt.Errorf("dependency resolution failed: %w", err))
		result.Duration = time.Since(start)
		return result
	}
	e.logger.Info().Msg("dependency resolution completed successfully")

	// Validate the pipeline
	e.logger.Info().Msg("validating operation pipeline")
	if err := pipelineInterface.Validate(ctx, fs); err != nil {
		e.logger.Info().Err(err).Msg("pipeline validation failed")
		result.Success = false
		result.Errors = append(result.Errors, fmt.Errorf("pipeline validation failed: %w", err))
		result.Duration = time.Since(start)
		return result
	}
	e.logger.Info().Msg("pipeline validation completed successfully")

	operationInterfaces := pipelineInterface.Operations()
	rollbackOps := make([]OperationInterface, 0, len(operationInterfaces))

	e.logger.Info().
		Int("operations_to_execute", len(operationInterfaces)).
		Msg("beginning operation execution")

	// Execute operations
	for i, opInterface := range operationInterfaces {
		// Convert to OperationInterface
		op, ok := opInterface.(OperationInterface)
		if !ok {
			e.logger.Info().
				Int("operation_index", i+1).
				Msg("operation does not implement OperationInterface - skipping")
			continue
		}

		e.logger.Info().
			Str("op_id", string(op.ID())).
			Str("op_type", op.Describe().Type).
			Str("path", op.Describe().Path).
			Int("operation_index", i+1).
			Int("total_operations", len(operationInterfaces)).
			Msg("executing operation")

		// Generate reverse operations if restorable mode is enabled
		var reverseOps []interface{}
		var backupData *core.BackupData
		var reverseErr error

		if opts.Restorable {
			e.logger.Debug().
				Str("op_id", string(op.ID())).
				Float64("remaining_budget_mb", budget.RemainingMB).
				Msg("generating reverse operations for backup")

			reverseOps, backupData, reverseErr = op.ReverseOps(ctx, fs, budget)
			if reverseErr != nil {
				e.logger.Warn().
					Str("op_id", string(op.ID())).
					Err(reverseErr).
					Msg("failed to generate reverse operations - operation will execute without backup")
			} else if backupData != nil {
				e.logger.Debug().
					Str("op_id", string(op.ID())).
					Float64("backup_size_mb", backupData.SizeMB).
					Float64("remaining_budget_mb", budget.RemainingMB).
					Str("backup_type", backupData.BackupType).
					Msg("backup data generated successfully")
			}
		}

		opStart := time.Now()
		err := op.ExecuteV2(ctx, execCtx, fs)
		opDuration := time.Since(opStart)

		opResult := core.OperationResult{
			OperationID:  op.ID(),
			Operation:    op,
			Duration:     opDuration,
			BackupData:   backupData,
			BackupSizeMB: 0,
		}

		if backupData != nil {
			opResult.BackupSizeMB = backupData.SizeMB
		}

		if err != nil {
			e.logger.Info().
				Str("op_id", string(op.ID())).
				Str("op_type", op.Describe().Type).
				Str("path", op.Describe().Path).
				Err(err).
				Dur("duration", opDuration).
				Msg("operation execution failed")

			opResult.Status = core.StatusFailure
			opResult.Error = err
			result.Success = false
			result.Errors = append(result.Errors, fmt.Errorf("operation %s failed: %w", op.ID(), err))

			// Restore budget if operation failed and backup was created
			if opts.Restorable && backupData != nil && budget != nil {
				budget.RestoreBackup(backupData.SizeMB)
				e.logger.Debug().
					Str("op_id", string(op.ID())).
					Float64("restored_budget_mb", backupData.SizeMB).
					Float64("remaining_budget_mb", budget.RemainingMB).
					Msg("restored backup budget due to operation failure")
			}
		} else {
			e.logger.Info().
				Str("op_id", string(op.ID())).
				Str("op_type", op.Describe().Type).
				Str("path", op.Describe().Path).
				Dur("duration", opDuration).
				Msg("operation execution completed successfully")

			opResult.Status = core.StatusSuccess
			rollbackOps = append(rollbackOps, op)

			// Add reverse operations to result if available
			if opts.Restorable && reverseOps != nil {
				result.RestoreOps = append(result.RestoreOps, reverseOps...)
				e.logger.Debug().
					Str("op_id", string(op.ID())).
					Int("reverse_ops_count", len(reverseOps)).
					Msg("added reverse operations for restoration")
			}
		}

		result.Operations = append(result.Operations, opResult)
	}

	result.Duration = time.Since(start)
	result.Rollback = e.createRollbackFunc(rollbackOps, fs)

	e.logger.Info().
		Bool("success", result.Success).
		Int("total_operations", len(operationInterfaces)).
		Int("successful_operations", len(rollbackOps)).
		Int("failed_operations", len(result.Errors)).
		Int("restore_operations", len(result.RestoreOps)).
		Dur("total_duration", result.Duration).
		Msg("execution completed")

	// Log budget usage summary
	if opts.Restorable && budget != nil {
		e.logger.Info().
			Float64("total_budget_mb", budget.TotalMB).
			Float64("used_budget_mb", budget.UsedMB).
			Float64("remaining_budget_mb", budget.RemainingMB).
			Msg("backup budget usage summary")
	}

	return result
}

// pipelineAdapter adapts Pipeline to PipelineInterface
type pipelineAdapter struct {
	pipeline Pipeline
}

func (pa *pipelineAdapter) Add(ops ...interface{}) error {
	return pa.pipeline.Add(ops...)
}

func (pa *pipelineAdapter) Operations() []interface{} {
	return pa.pipeline.Operations()
}

func (pa *pipelineAdapter) Resolve() error {
	return pa.pipeline.Resolve()
}

func (pa *pipelineAdapter) ResolvePrerequisites(resolver core.PrerequisiteResolver, fs interface{}) error {
	return pa.pipeline.ResolvePrerequisites(resolver, fs)
}

func (pa *pipelineAdapter) Validate(ctx context.Context, fs interface{}) error {
	return pa.pipeline.Validate(ctx, fs)
}

// createRollbackFunc creates a rollback function that can undo executed operations
func (e *Executor) createRollbackFunc(executedOps []OperationInterface, fsys interface{}) func(context.Context) error {
	if len(executedOps) == 0 {
		return func(ctx context.Context) error { return nil }
	}

	return func(ctx context.Context) error {
		// Rollback in reverse order
		var rollbackErrors []error
		for i := len(executedOps) - 1; i >= 0; i-- {
			op := executedOps[i]
			if err := op.Rollback(ctx, fsys); err != nil {
				rollbackErrors = append(rollbackErrors, fmt.Errorf("rollback failed for operation %s: %w", op.ID(), err))
			}
		}

		if len(rollbackErrors) > 0 {
			return fmt.Errorf("rollback errors: %v", rollbackErrors)
		}
		return nil
	}
}
