package operations

import (
	"fmt"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// Factory creates operations based on type.
type Factory struct {
	// Can add configuration options here if needed
}

// NewFactory creates a new operation factory.
func NewFactory() *Factory {
	return &Factory{}
}

// CreateOperation creates an operation based on the type.
func (f *Factory) CreateOperation(id core.OperationID, opType string, path string) (Operation, error) {
	switch opType {
	case "create_file":
		return NewCreateFileOperation(id, path), nil
	case "create_directory":
		return NewCreateDirectoryOperation(id, path), nil
	case "copy":
		return NewCopyOperation(id, path), nil
	case "move":
		return NewMoveOperation(id, path), nil
	case "delete":
		return NewDeleteOperation(id, path), nil
	case "create_symlink":
		return NewCreateSymlinkOperation(id, path), nil
	case "create_archive":
		return NewCreateArchiveOperation(id, path), nil
	case "unarchive":
		return NewUnarchiveOperation(id, path), nil
	default:
		return nil, fmt.Errorf("unknown operation type: %s", opType)
	}
}

// SetItemForOperation sets an item on an operation.
// This is a helper for the transition period.
func (f *Factory) SetItemForOperation(op Operation, item interface{}) error {
	if op == nil {
		return fmt.Errorf("operation is nil")
	}

	op.SetItem(item)
	return nil
}
