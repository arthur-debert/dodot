package synthfs

import (
	"context"
	"io/fs"

	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// SimpleBatch provides a fluent API for building and executing multiple operations.
//
// This is a convenience wrapper that doesn't require a registry and provides method
// chaining for readable operation sequences. Operations are executed in the order
// they were added, with automatic dependency resolution.
//
// Example usage:
//
//	fs := synthfs.NewOSFileSystem("/tmp")
//	batch := synthfs.NewSimpleBatch(fs)
//
//	err := batch.
//		CreateDir("project", 0755).
//		CreateDir("project/src", 0755).
//		WriteFile("project/README.md", []byte("# My Project"), 0644).
//		WriteFile("project/src/main.go", []byte("package main"), 0644).
//		Copy("template.conf", "project/config.conf").
//		Execute(ctx)
//
//	if err != nil {
//		log.Fatal(err)
//	}
//
// For operations requiring rollback capability:
//
//	result, err := batch.ExecuteWithRollback(ctx)
//	if err != nil && result.Rollback != nil {
//		rollbackErr := result.Rollback(ctx)
//		if rollbackErr != nil {
//			log.Printf("Rollback failed: %v", rollbackErr)
//		}
//	}
type SimpleBatch struct {
	fs         filesystem.FullFileSystem
	operations []Operation
	ctx        context.Context
}

// NewSimpleBatch creates a new simple batch for the given filesystem
func NewSimpleBatch(fs filesystem.FullFileSystem) *SimpleBatch {
	return &SimpleBatch{
		fs:         fs,
		operations: []Operation{},
		ctx:        context.Background(),
	}
}

// WithContext sets the context for batch execution
func (sb *SimpleBatch) WithContext(ctx context.Context) *SimpleBatch {
	sb.ctx = ctx
	return sb
}

// CreateDir adds a directory creation operation to the batch
func (sb *SimpleBatch) CreateDir(path string, mode fs.FileMode) *SimpleBatch {
	op := New().CreateDir(path, mode)
	sb.operations = append(sb.operations, op)
	return sb
}

// WriteFile adds a file write operation to the batch
func (sb *SimpleBatch) WriteFile(path string, content []byte, mode fs.FileMode) *SimpleBatch {
	op := New().CreateFile(path, content, mode)
	sb.operations = append(sb.operations, op)
	return sb
}

// Copy adds a copy operation to the batch
func (sb *SimpleBatch) Copy(src, dst string) *SimpleBatch {
	op := New().Copy(src, dst)
	sb.operations = append(sb.operations, op)
	return sb
}

// Move adds a move operation to the batch
func (sb *SimpleBatch) Move(src, dst string) *SimpleBatch {
	op := New().Move(src, dst)
	sb.operations = append(sb.operations, op)
	return sb
}

// Delete adds a delete operation to the batch
func (sb *SimpleBatch) Delete(path string) *SimpleBatch {
	op := New().Delete(path)
	sb.operations = append(sb.operations, op)
	return sb
}

// CreateSymlink adds a symlink creation operation to the batch
func (sb *SimpleBatch) CreateSymlink(target, linkPath string) *SimpleBatch {
	op := New().CreateSymlink(target, linkPath)
	sb.operations = append(sb.operations, op)
	return sb
}

// Execute runs all operations in the batch
func (sb *SimpleBatch) Execute() error {
	_, err := Run(sb.ctx, sb.fs, sb.operations...)
	return err
}

// ExecuteWithRollback runs all operations and attempts rollback on failure
func (sb *SimpleBatch) ExecuteWithRollback() error {
	options := DefaultPipelineOptions()
	options.RollbackOnError = true
	_, err := RunWithOptions(sb.ctx, sb.fs, options, sb.operations...)
	return err
}

// Operations returns the list of operations in the batch
func (sb *SimpleBatch) Operations() []Operation {
	return sb.operations
}

// Clear removes all operations from the batch
func (sb *SimpleBatch) Clear() *SimpleBatch {
	sb.operations = []Operation{}
	return sb
}
