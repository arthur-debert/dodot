package synthfs

import (
	"context"
	"io/fs"

	"github.com/arthur-debert/synthfs/pkg/synthfs/batch"
	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// Batch is a wrapper around batch.Batch that provides convenience methods for operation results
type Batch struct {
	impl batch.Batch
}

// NewBatch creates a new batch with the clean implementation that has prerequisite resolution enabled by default
func NewBatch(fs interface{}) *Batch {
	return &Batch{
		impl: batch.NewBatch(fs, NewOperationRegistry()),
	}
}

// Operations returns all operations currently in the batch.
func (b *Batch) Operations() []interface{} {
	return b.impl.Operations()
}

// Operation creation methods

// CreateDir adds a directory creation operation to the batch.
func (b *Batch) CreateDir(path string, mode fs.FileMode, metadata ...map[string]interface{}) (interface{}, error) {
	return b.impl.CreateDir(path, mode, metadata...)
}

// CreateFile adds a file creation operation to the batch.
func (b *Batch) CreateFile(path string, content []byte, mode fs.FileMode, metadata ...map[string]interface{}) (interface{}, error) {
	return b.impl.CreateFile(path, content, mode, metadata...)
}

// Copy adds a copy operation to the batch.
func (b *Batch) Copy(src, dst string, metadata ...map[string]interface{}) (interface{}, error) {
	return b.impl.Copy(src, dst, metadata...)
}

// Move adds a move operation to the batch.
func (b *Batch) Move(src, dst string, metadata ...map[string]interface{}) (interface{}, error) {
	return b.impl.Move(src, dst, metadata...)
}

// Delete adds a delete operation to the batch.
func (b *Batch) Delete(path string, metadata ...map[string]interface{}) (interface{}, error) {
	return b.impl.Delete(path, metadata...)
}

// CreateSymlink adds a symbolic link creation operation to the batch.
func (b *Batch) CreateSymlink(target, linkPath string, metadata ...map[string]interface{}) (interface{}, error) {
	return b.impl.CreateSymlink(target, linkPath, metadata...)
}

// CreateArchive adds an archive creation operation to the batch.
func (b *Batch) CreateArchive(archivePath string, format interface{}, sources []string, metadata ...map[string]interface{}) (interface{}, error) {
	return b.impl.CreateArchive(archivePath, format, sources, metadata...)
}

// Unarchive adds an unarchive operation to the batch.
func (b *Batch) Unarchive(archivePath, extractPath string, metadata ...map[string]interface{}) (interface{}, error) {
	return b.impl.Unarchive(archivePath, extractPath, metadata...)
}

// UnarchiveWithPatterns adds an unarchive operation with pattern filtering to the batch.
func (b *Batch) UnarchiveWithPatterns(archivePath, extractPath string, patterns []string, metadata ...map[string]interface{}) (interface{}, error) {
	return b.impl.UnarchiveWithPatterns(archivePath, extractPath, patterns, metadata...)
}

// Configuration methods

// WithFileSystem sets the filesystem for the batch operations.
func (b *Batch) WithFileSystem(fs interface{}) *Batch {
	b.impl = b.impl.WithFileSystem(fs)
	return b
}

// WithContext sets the context for the batch operations.
func (b *Batch) WithContext(ctx context.Context) *Batch {
	b.impl = b.impl.WithContext(ctx)
	return b
}

// WithRegistry sets a custom operation registry for the batch.
func (b *Batch) WithRegistry(registry core.OperationFactory) *Batch {
	b.impl = b.impl.WithRegistry(registry)
	return b
}

// WithLogger sets the logger for the batch.
func (b *Batch) WithLogger(logger core.Logger) *Batch {
	b.impl = b.impl.WithLogger(logger)
	return b
}

// WithMetadata sets metadata for the batch.
func (b *Batch) WithMetadata(metadata map[string]interface{}) *Batch {
	b.impl = b.impl.WithMetadata(metadata)
	return b
}

// Execution methods

// Run runs all operations in the batch using default options with prerequisite resolution.
func (b *Batch) Run() (*Result, error) {
	batchResult, err := b.impl.Run()
	if err != nil {
		return nil, err
	}
	return ConvertBatchResult(batchResult), nil
}

// RunWithOptions runs all operations in the batch with specified options.
func (b *Batch) RunWithOptions(opts interface{}) (*Result, error) {
	batchResult, err := b.impl.RunWithOptions(opts)
	if err != nil {
		return nil, err
	}
	return ConvertBatchResult(batchResult), nil
}

// RunRestorable runs all operations with backup enabled using the default 10MB budget.
func (b *Batch) RunRestorable() (*Result, error) {
	batchResult, err := b.impl.RunRestorable()
	if err != nil {
		return nil, err
	}
	return ConvertBatchResult(batchResult), nil
}

// RunRestorableWithBudget runs all operations with backup enabled using a custom budget.
func (b *Batch) RunRestorableWithBudget(maxBackupMB int) (*Result, error) {
	batchResult, err := b.impl.RunRestorableWithBudget(maxBackupMB)
	if err != nil {
		return nil, err
	}
	return ConvertBatchResult(batchResult), nil
}

// Result conversion utilities

// ConvertBatchResult converts a batch result interface{} to our typed Result
func ConvertBatchResult(batchResult interface{}) *Result {
	if r, ok := batchResult.(interface {
		IsSuccess() bool
		GetOperations() []interface{}
		GetRestoreOps() []interface{}
		GetDuration() interface{}
		GetError() error
		GetBudget() interface{}
		GetRollback() interface{}
		GetMetadata() map[string]interface{}
	}); ok {
		return &Result{
			success:    r.IsSuccess(),
			operations: r.GetOperations(),
			restoreOps: r.GetRestoreOps(),
			duration:   r.GetDuration(),
			err:        r.GetError(),
			budget:     r.GetBudget(),
			rollback:   r.GetRollback(),
			metadata:   r.GetMetadata(),
		}
	}

	// Fallback for basic result structure
	return &Result{
		success:    true,
		operations: []interface{}{},
		restoreOps: []interface{}{},
		duration:   nil,
		err:        nil,
		budget:     nil,
		rollback:   nil,
		metadata:   nil,
	}
}

// Result represents the outcome of executing a batch of operations
type Result struct {
	success    bool
	operations []interface{}
	restoreOps []interface{}
	duration   interface{}
	err        error
	budget     interface{}
	rollback   interface{}
	metadata   map[string]interface{}
}

// IsSuccess returns whether the batch execution was successful
func (r *Result) IsSuccess() bool {
	return r.success
}

// GetOperations returns the operations that were executed
func (r *Result) GetOperations() []interface{} {
	return r.operations
}

// GetRestoreOps returns operations needed to restore the state
func (r *Result) GetRestoreOps() []interface{} {
	return r.restoreOps
}

// GetDuration returns the execution duration
func (r *Result) GetDuration() interface{} {
	return r.duration
}

// GetError returns any error that occurred during execution
func (r *Result) GetError() error {
	return r.err
}

// GetBudget returns budget information if available
func (r *Result) GetBudget() interface{} {
	return r.budget
}

// GetRollback returns rollback information if available
func (r *Result) GetRollback() interface{} {
	return r.rollback
}

// GetMetadata returns the user-defined metadata for the batch
func (r *Result) GetMetadata() map[string]interface{} {
	return r.metadata
}
