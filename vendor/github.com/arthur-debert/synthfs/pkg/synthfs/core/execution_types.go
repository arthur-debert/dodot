package core

import (
	"context"
	"time"
)

// PipelineOptions defines options for pipeline execution.
type PipelineOptions struct {
	// DryRun, if true, will cause the pipeline to go through validation
	// and dependency resolution, but will not execute any operations.
	DryRun bool

	// RollbackOnError, if true, will cause the pipeline to attempt a rollback
	// of successful operations if a subsequent operation fails.
	RollbackOnError bool

	// ContinueOnError, if true, will cause the pipeline to continue
	// executing subsequent operations even if one fails.
	ContinueOnError bool

	// MaxConcurrent is the maximum number of operations to execute concurrently.
	// A value of 0 or 1 means sequential execution.
	MaxConcurrent int

	// Restorable, if true, enables the backup mechanism for rollback.
	Restorable bool

	// MaxBackupSizeMB is the maximum memory budget for backups in megabytes.
	MaxBackupSizeMB int

	// ResolvePrerequisites, if true, automatically resolves prerequisites.
	ResolvePrerequisites bool

	// UseSimpleBatch, if true, uses the simple batch execution model.
	UseSimpleBatch bool
}

// OperationResult holds the outcome of a single operation's execution
type OperationResult struct {
	OperationID  OperationID
	Operation    interface{} // The operation that was executed (interface to avoid circular dep)
	Status       OperationStatus
	Error        error
	Duration     time.Duration
	BackupData   *BackupData            // Backup data for restoration (only if restorable=true)
	BackupSizeMB float64                // Actual backup size consumed
	Metadata     map[string]interface{} // User-defined metadata for the operation
}

// Result holds the overall outcome of running a pipeline of operations
type Result struct {
	Success    bool              // True if all operations were successful
	Operations []OperationResult // Results for each operation attempted
	Duration   time.Duration
	Errors     []error                     // Aggregated errors from operations that failed
	Rollback   func(context.Context) error // Rollback function for failed transactions

	// Enhanced restoration functionality
	Budget     *BackupBudget          // Backup budget information (only if restorable=true)
	RestoreOps []interface{}          // Generated reverse operations for restoration
	Metadata   map[string]interface{} // User-defined metadata for the batch/pipeline
}
