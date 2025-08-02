package core

import (
	"context"
	"time"
)

// PipelineOptions controls how operations are executed
type PipelineOptions struct {
	Restorable           bool // Whether to enable reversible operations with backup
	MaxBackupSizeMB      int  // Maximum backup size in MB (default: 10MB)
	ResolvePrerequisites bool // Whether to resolve prerequisites like parent directories (default: true)
	UseSimpleBatch       bool // Whether to use SimpleBatch implementation (default: true)
}

// OperationResult holds the outcome of a single operation's execution
type OperationResult struct {
	OperationID  OperationID
	Operation    interface{} // The operation that was executed (interface to avoid circular dep)
	Status       OperationStatus
	Error        error
	Duration     time.Duration
	BackupData   *BackupData // Backup data for restoration (only if restorable=true)
	BackupSizeMB float64     // Actual backup size consumed
}

// Result holds the overall outcome of running a pipeline of operations
type Result struct {
	Success    bool              // True if all operations were successful
	Operations []OperationResult // Results for each operation attempted
	Duration   time.Duration
	Errors     []error                     // Aggregated errors from operations that failed
	Rollback   func(context.Context) error // Rollback function for failed transactions

	// Enhanced restoration functionality
	Budget     *BackupBudget // Backup budget information (only if restorable=true)
	RestoreOps []interface{} // Generated reverse operations for restoration
}
