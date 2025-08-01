package types

// OperationType defines the type of file system operation
type OperationType string

const (
	// OperationCreateSymlink creates a symbolic link
	OperationCreateSymlink OperationType = "create_symlink"

	// OperationCreateDir creates a directory
	OperationCreateDir OperationType = "create_dir"

	// OperationCopyFile copies a file
	OperationCopyFile OperationType = "copy_file"

	// OperationWriteFile writes content to a file
	OperationWriteFile OperationType = "write_file"

	// OperationDeleteFile deletes a file
	OperationDeleteFile OperationType = "delete_file"

	// OperationBackupFile creates a backup of a file
	OperationBackupFile OperationType = "backup_file"

	// OperationReadFile reads file contents
	OperationReadFile OperationType = "read_file"

	// OperationChecksum calculates file checksum
	OperationChecksum OperationType = "checksum"
)

// OperationStatus defines the state of an operation
type OperationStatus string

const (
	// StatusReady means the operation is ready to be executed
	StatusReady OperationStatus = "ready"
	// StatusSkipped means the operation was skipped (e.g., idempotent action)
	StatusSkipped OperationStatus = "skipped"
	// StatusConflict means the operation cannot be performed due to a conflict
	StatusConflict OperationStatus = "conflict"
	// StatusError means the operation resulted in an error
	StatusError OperationStatus = "error"
)

// Operation represents a low-level file system operation
// These are the actual operations that will be performed by synthfs
type Operation struct {
	// Type is the type of operation
	Type OperationType

	// Source is the source path (for symlinks, copies)
	Source string

	// Target is the target path
	Target string

	// Content is the content to write (for write operations)
	Content string

	// Mode is the file permissions (optional)
	Mode *uint32

	// Description is a human-readable description
	Description string

	// Status is the current state of the operation
	Status OperationStatus
}
