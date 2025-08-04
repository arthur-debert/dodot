package synthfs

import (
	"context"
	"io/fs"

	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// Direct execution methods for simple use cases

// WriteFile writes a file directly to the filesystem.
//
// This is a convenience function for simple file writing operations.
// For batch operations or complex workflows, consider using SimpleBatch or Pipeline.
//
// Example:
//
//	fs := synthfs.NewOSFileSystem("/tmp")
//	err := synthfs.WriteFile(ctx, fs, "config.json", []byte(`{"debug": true}`), 0644)
//	if err != nil {
//		log.Fatal(err)
//	}
//
// For multiple related operations, use SimpleBatch:
//
//	batch := synthfs.NewSimpleBatch(fs)
//	err := batch.
//		CreateDir("project", 0755).
//		WriteFile("project/config.json", []byte(`{}`), 0644).
//		WriteFile("project/README.md", []byte("# Project"), 0644).
//		Execute(ctx)
func WriteFile(ctx context.Context, fs filesystem.FileSystem, path string, content []byte, mode fs.FileMode) error {
	op := New().CreateFile(path, content, mode)
	return executeDirectOp(ctx, fs, op)
}

// MkdirAll creates a directory and all necessary parents.
//
// This function automatically creates parent directories as needed, similar to `mkdir -p`.
// It's ideal for single directory creation; for complex directory structures, consider
// using patterns like CreateStructure or SimpleBatch.
//
// Example:
//
//	fs := synthfs.NewOSFileSystem("/")
//	err := synthfs.MkdirAll(ctx, fs, "/tmp/project/data/logs", 0755)
//	if err != nil {
//		log.Fatal(err)
//	}
//
// For creating multiple directories with files, use SimpleBatch:
//
//	batch := synthfs.NewSimpleBatch(fs)
//	err := batch.
//		CreateDir("app/config", 0755).
//		CreateDir("app/data", 0755).
//		CreateDir("app/logs", 0755).
//		WriteFile("app/config/app.conf", configData, 0644).
//		Execute(ctx)
func MkdirAll(ctx context.Context, fs filesystem.FileSystem, path string, mode fs.FileMode) error {
	op := New().CreateDir(path, mode)
	return executeDirectOp(ctx, fs, op)
}

// Remove removes a file or empty directory.
//
// This function removes a single file or empty directory. For recursive directory
// removal or batch deletions, consider using the Delete operation in a Pipeline
// or use patterns like CopyTree with filters.
//
// Example:
//
//	fs := synthfs.NewOSFileSystem("/tmp")
//	err := synthfs.Remove(ctx, fs, "temp-file.txt")
//	if err != nil {
//		log.Fatal(err)
//	}
//
// For removing multiple files or directories, use SimpleBatch:
//
//	batch := synthfs.NewSimpleBatch(fs)
//	err := batch.
//		Delete("temp1.txt").
//		Delete("temp2.txt").
//		Delete("empty-dir").
//		Execute(ctx)
//
// For recursive directory removal, use the low-level API:
//
//	sfs := synthfs.New()
//	pipeline := synthfs.BuildPipeline(sfs.Delete("directory-tree"))
//	result, err := pipeline.Execute(ctx, fs)
func Remove(ctx context.Context, fs filesystem.FileSystem, path string) error {
	op := New().Delete(path)
	return executeDirectOp(ctx, fs, op)
}

// executeDirectOp executes a single operation directly
func executeDirectOp(ctx context.Context, fs filesystem.FileSystem, op Operation) error {
	// Get operation description for error context
	desc := op.Describe()
	action := getOperationAction(desc.Type)

	// Validate first
	if err := op.Validate(ctx, fs); err != nil {
		return WrapOperationError(op, action, err)
	}

	// Execute
	if err := op.Execute(ctx, fs); err != nil {
		return WrapOperationError(op, action, err)
	}

	return nil
}
