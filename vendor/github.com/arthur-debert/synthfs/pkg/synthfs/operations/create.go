package operations

import (
	"context"
	"fmt"
	"io"
	"io/fs"
	"path/filepath"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// CreateFileOperation represents a file creation operation with clean interfaces.
type CreateFileOperation struct {
	*BaseOperation
}

// NewCreateFileOperation creates a new file creation operation.
func NewCreateFileOperation(id core.OperationID, path string) *CreateFileOperation {
	return &CreateFileOperation{
		BaseOperation: NewBaseOperation(id, "create_file", path),
	}
}

// Prerequisites returns the prerequisites for creating a file.
func (op *CreateFileOperation) Prerequisites() []core.Prerequisite {
	var prereqs []core.Prerequisite

	// Always need parent directory to exist (even if it's current directory)
	prereqs = append(prereqs, core.NewParentDirPrerequisite(op.description.Path))

	// Need no conflict with existing files
	prereqs = append(prereqs, core.NewNoConflictPrerequisite(op.description.Path))

	return prereqs
}

// Execute creates the file. The filesystem interface is generic to avoid coupling.
func (op *CreateFileOperation) Execute(ctx context.Context, fsys interface{}) error {
	item := op.GetItem()
	if item == nil {
		return fmt.Errorf("create_file operation requires an item")
	}

	// The item should implement our ItemInterface
	fileItem, ok := item.(ItemInterface)
	if !ok {
		return fmt.Errorf("item does not implement ItemInterface")
	}

	// Get filesystem methods through interface assertions
	// This allows us to work with any filesystem implementation
	writeFile, ok := getWriteFileMethod(fsys)
	if !ok {
		return fmt.Errorf("filesystem does not support WriteFile")
	}

	mkdirAll, ok := getMkdirAllMethod(fsys)
	if !ok {
		return fmt.Errorf("filesystem does not support MkdirAll")
	}

	// Create parent directory if needed
	dir := filepath.Dir(fileItem.Path())
	if dir != "." && dir != "/" {
		if err := mkdirAll(dir, 0755); err != nil {
			return fmt.Errorf("failed to create parent directory: %w", err)
		}
	}

	// Get content and mode from item
	var content []byte
	var mode interface{} = fs.FileMode(0644) // Default

	// Try to get content from item
	if contentGetter, ok := item.(interface{ Content() []byte }); ok {
		content = contentGetter.Content()
	}

	// Try to get mode from item
	if modeGetter, ok := item.(interface{ Mode() fs.FileMode }); ok {
		mode = modeGetter.Mode()
	}

	// Write the file
	if err := writeFile(fileItem.Path(), content, mode); err != nil {
		return fmt.Errorf("failed to write file: %w", err)
	}

	return nil
}

// ExecuteV2 performs the file creation with execution context support.
func (op *CreateFileOperation) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	// Convert context
	context, ok := ctx.(context.Context)
	if !ok {
		return fmt.Errorf("invalid context type")
	}

	// Call the operation's Execute method with proper event handling
	return executeWithEvents(op, context, execCtx, fsys, op.Execute)
}

// ValidateV2 checks if the create file operation can be performed using ExecutionContext.
func (op *CreateFileOperation) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return validateV2Helper(op, ctx, execCtx, fsys)
}

// Validate checks if the file can be created.
func (op *CreateFileOperation) Validate(ctx context.Context, fsys interface{}) error {
	// First do base validation
	if err := op.BaseOperation.Validate(ctx, fsys); err != nil {
		return err
	}

	item := op.GetItem()
	if item == nil {
		return &core.ValidationError{
			OperationID:   op.ID(),
			OperationDesc: op.Describe(),
			Reason:        "no item provided for create_file operation",
		}
	}

	// Check if item is a directory when we expect a file
	if typeGetter, ok := item.(interface{ Type() string }); ok {
		if typeGetter.Type() == "directory" {
			return &core.ValidationError{
				OperationID:   op.ID(),
				OperationDesc: op.Describe(),
				Reason:        "expected file item but got directory",
			}
		}
	}

	// Check if item implements IsDir
	if dirChecker, ok := item.(interface{ IsDir() bool }); ok {
		if dirChecker.IsDir() {
			return &core.ValidationError{
				OperationID:   op.ID(),
				OperationDesc: op.Describe(),
				Reason:        "cannot create file: item IsDir() returns true",
			}
		}
	}

	// Check if filesystem supports required operations
	if _, ok := getWriteFileMethod(fsys); !ok {
		return &core.ValidationError{
			OperationID:   op.ID(),
			OperationDesc: op.Describe(),
			Reason:        "filesystem does not support WriteFile",
		}
	}

	return nil
}

// Helper functions to extract methods from filesystem interface
func getWriteFileMethod(fsys interface{}) (func(string, []byte, interface{}) error, bool) {
	// Try interface{} version first
	type writeFSInterface interface {
		WriteFile(name string, data []byte, perm interface{}) error
	}

	if fs, ok := fsys.(writeFSInterface); ok {
		return fs.WriteFile, true
	}

	// Try fs.FileMode version
	type writeFSFileMode interface {
		WriteFile(name string, data []byte, perm fs.FileMode) error
	}

	if fsFileMode, ok := fsys.(writeFSFileMode); ok {
		// Wrap to convert interface{} to fs.FileMode
		return func(name string, data []byte, perm interface{}) error {
			fileMode, ok := perm.(fs.FileMode)
			if !ok {
				// Try to convert from other types
				if mode, ok := perm.(int); ok {
					fileMode = fs.FileMode(mode)
				} else {
					fileMode = 0644 // Default
				}
			}
			return fsFileMode.WriteFile(name, data, fileMode)
		}, true
	}

	return nil, false
}

func getMkdirAllMethod(fsys interface{}) (func(string, interface{}) error, bool) {
	// Try interface{} version first
	type mkdirFSInterface interface {
		MkdirAll(path string, perm interface{}) error
	}

	if fs, ok := fsys.(mkdirFSInterface); ok {
		return fs.MkdirAll, true
	}

	// Try fs.FileMode version
	type mkdirFSFileMode interface {
		MkdirAll(path string, perm fs.FileMode) error
	}

	if fsFileMode, ok := fsys.(mkdirFSFileMode); ok {
		// Wrap to convert interface{} to fs.FileMode
		return func(path string, perm interface{}) error {
			fileMode, ok := perm.(fs.FileMode)
			if !ok {
				// Try to convert from other types
				if mode, ok := perm.(int); ok {
					fileMode = fs.FileMode(mode)
				} else {
					fileMode = 0755 // Default for directories
				}
			}
			return fsFileMode.MkdirAll(path, fileMode)
		}, true
	}

	return nil, false
}

// Rollback removes the created file
func (op *CreateFileOperation) Rollback(ctx context.Context, fsys interface{}) error {
	remove, ok := getRemoveMethod(fsys)
	if !ok {
		return fmt.Errorf("filesystem does not support Remove")
	}

	// Remove the created file
	return remove(op.description.Path)
}

// ReverseOps for CreateFileOperation - returns a delete operation
func (op *CreateFileOperation) ReverseOps(ctx context.Context, fsys interface{}, budget interface{}) ([]interface{}, interface{}, error) {
	// If file doesn't exist, reverse op is to delete the created file
	stat, ok := getStatMethod(fsys)
	if !ok {
		return nil, nil, fmt.Errorf("filesystem does not support Stat")
	}

	info, err := stat(op.description.Path)
	if err != nil {
		// File does not exist, so reverse is a simple delete
		return []interface{}{NewDeleteOperation(core.OperationID(fmt.Sprintf("reverse_%s", op.ID())), op.description.Path)}, nil, nil
	}

	// File exists, so we need to back it up
	open, ok := getOpenMethod(fsys)
	if !ok {
		return nil, nil, fmt.Errorf("filesystem does not support Open")
	}

	file, err := open(op.description.Path)
	if err != nil {
		return nil, nil, fmt.Errorf("failed to open existing file for backup: %w", err)
	}
	if closer, ok := file.(io.Closer); ok {
		defer func() {
			if closeErr := closer.Close(); closeErr != nil {
				// Log error but don't fail the operation
				// The file was already read successfully
				_ = closeErr // Explicitly ignore the error
			}
		}()
	}

	content, err := io.ReadAll(file.(io.Reader))
	if err != nil {
		return nil, nil, fmt.Errorf("failed to read existing file for backup: %w", err)
	}

	backupData := &core.BackupData{
		OperationID:   op.ID(),
		BackupType:    "file",
		OriginalPath:  op.description.Path,
		BackupContent: content,
		SizeMB:        float64(len(content)) / (1024 * 1024),
		BackupTime:    time.Now(),
		Metadata: map[string]interface{}{
			"mode": info.(fs.FileInfo).Mode(),
		},
	}

	// Create a file creation operation to restore the backed up content
	reverseOp := NewCreateFileOperation(
		core.OperationID(fmt.Sprintf("reverse_%s", op.ID())),
		op.description.Path,
	)
	reverseOp.SetItem(&MinimalItem{
		path:     op.description.Path,
		itemType: "file",
		content:  content,
		mode:     info.(fs.FileInfo).Mode(),
	})

	return []interface{}{reverseOp}, backupData, nil
}

// ReverseOps for CreateDirectoryOperation - returns a delete operation
func (op *CreateDirectoryOperation) ReverseOps(ctx context.Context, fsys interface{}, budget interface{}) ([]interface{}, interface{}, error) {
	// Create a delete operation to remove the directory
	reverseOp := NewDeleteOperation(
		core.OperationID(fmt.Sprintf("reverse_%s", op.ID())),
		op.description.Path,
	)

	return []interface{}{reverseOp}, nil, nil
}
