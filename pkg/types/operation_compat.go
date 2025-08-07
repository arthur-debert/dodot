package types

import "time"

// Temporary Operation compatibility types for DirectExecutor
// These are minimal types to keep DirectExecutor working during transition
// TODO: Remove once DirectExecutor is refactored to work with PowerUp results

// OperationType defines the type of file system operation (minimal for DirectExecutor)
type OperationType string

const (
	OperationCreateSymlink OperationType = "create_symlink"
	OperationCreateDir     OperationType = "create_dir"
	OperationWriteFile     OperationType = "write_file"
	OperationReadFile      OperationType = "read_file"
	OperationChecksum      OperationType = "checksum"
	OperationExecute       OperationType = "execute"
)

// OperationStatus defines the state of an operation (minimal for DirectExecutor)
type OperationStatus string

const (
	StatusReady    OperationStatus = "ready"
	StatusSkipped  OperationStatus = "skipped"
	StatusConflict OperationStatus = "conflict"
	StatusError    OperationStatus = "error"
	StatusUnknown  OperationStatus = "unknown"
)

// Operation represents a low-level file system operation (minimal for DirectExecutor)
type Operation struct {
	Type        OperationType
	Source      string
	Target      string
	Description string
	Pack        string
	PowerUp     string
}

// OperationResult tracks the result of a single operation execution (minimal for DirectExecutor)
type OperationResult struct {
	Operation *Operation
	Status    OperationStatus
	Error     error
	StartTime time.Time
	EndTime   time.Time
}
