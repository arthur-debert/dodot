package operations

import (
	"context"
	"fmt"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// BaseOperation provides a base implementation of the Operation interface.
// Operations are created complete and immutable - no post-creation modification.
type BaseOperation struct {
	id           core.OperationID
	dependencies []core.OperationID
	description  core.OperationDesc
	item         interface{}            // Generic item interface
	srcPath      string                 // For Copy/Move operations
	dstPath      string                 // For Copy/Move operations
	checksums    map[string]interface{} // Generic checksum storage
}

// NewBaseOperation creates a new base operation.
func NewBaseOperation(id core.OperationID, descType string, path string) *BaseOperation {
	return &BaseOperation{
		id: id,
		description: core.OperationDesc{
			Type:    descType,
			Path:    path,
			Details: make(map[string]interface{}),
		},
		dependencies: []core.OperationID{},
		checksums:    make(map[string]interface{}),
	}
}

// ID returns the operation's ID.
func (op *BaseOperation) ID() core.OperationID {
	return op.id
}

// Dependencies returns the list of operation dependencies.
func (op *BaseOperation) Dependencies() []core.OperationID {
	return op.dependencies
}

// Conflicts returns an empty list (conflicts not implemented yet).
func (op *BaseOperation) Conflicts() []core.OperationID {
	return nil
}

// Prerequisites returns an empty list of prerequisites (default implementation).
// Concrete operations should override this method to declare their prerequisites.
func (op *BaseOperation) Prerequisites() []core.Prerequisite {
	return nil
}

// Describe returns the operation's description.
func (op *BaseOperation) Describe() core.OperationDesc {
	return op.description
}

// GetItem returns the item associated with this operation.
func (op *BaseOperation) GetItem() interface{} {
	return op.item
}

// SetItem sets the item for operations.
func (op *BaseOperation) SetItem(item interface{}) {
	op.item = item
}

// GetPaths returns source and destination paths.
func (op *BaseOperation) GetPaths() (src, dst string) {
	return op.srcPath, op.dstPath
}

// SetPaths sets source and destination paths for Copy/Move operations.
func (op *BaseOperation) SetPaths(src, dst string) {
	op.srcPath = src
	op.dstPath = dst
}

// AddDependency adds a dependency to the operation.
func (op *BaseOperation) AddDependency(depID core.OperationID) {
	op.dependencies = append(op.dependencies, depID)
}

// SetDescriptionDetail sets a detail in the operation's description.
func (op *BaseOperation) SetDescriptionDetail(key string, value interface{}) {
	if op.description.Details == nil {
		op.description.Details = make(map[string]interface{})
	}
	op.description.Details[key] = value
}

// SetChecksum stores a checksum record for a file path
func (op *BaseOperation) SetChecksum(path string, checksum interface{}) {
	if op.checksums == nil {
		op.checksums = make(map[string]interface{})
	}
	op.checksums[path] = checksum
}

// GetChecksum retrieves a checksum record for a file path
func (op *BaseOperation) GetChecksum(path string) interface{} {
	if op.checksums == nil {
		return nil
	}
	return op.checksums[path]
}

// GetAllChecksums returns all checksum records
func (op *BaseOperation) GetAllChecksums() map[string]interface{} {
	return op.checksums
}

// Execute performs the actual filesystem operation.
// Subclasses should override this method.
func (op *BaseOperation) Execute(ctx context.Context, fsys interface{}) error {
	return fmt.Errorf("Execute not implemented for operation type: %s", op.description.Type)
}

// Validate checks if the operation can be performed.
// Subclasses should override this method.
func (op *BaseOperation) Validate(ctx context.Context, fsys interface{}) error {
	// Basic validation: reject empty paths
	if op.description.Path == "" {
		return &core.ValidationError{
			OperationID:   op.ID(),
			OperationDesc: op.Describe(),
			Reason:        "path cannot be empty",
			Cause:         nil,
		}
	}
	return nil
}

// Rollback attempts to undo the effects of the Execute method.
// Subclasses should override this method.
func (op *BaseOperation) Rollback(ctx context.Context, fsys interface{}) error {
	return fmt.Errorf("Rollback not implemented for operation type: %s", op.description.Type)
}

// ReverseOps generates operations that would undo this operation's effects.
// Subclasses should override this method.
func (op *BaseOperation) ReverseOps(ctx context.Context, fsys interface{}, budget interface{}) ([]interface{}, interface{}, error) {
	return nil, nil, fmt.Errorf("ReverseOps not implemented for operation type: %s", op.description.Type)
}

// Additional methods to satisfy the main package Operation interface
// Note: GetItem, GetChecksum, and GetAllChecksums are already defined above

// ExecuteV2 performs the operation using ExecutionContext.
// IMPORTANT: This base implementation should NOT be used. Each concrete operation
// type must override this method to ensure proper method dispatch.
func (op *BaseOperation) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	// This is a fallback that should not be reached if operations are properly implemented
	return fmt.Errorf("ExecuteV2 not properly implemented for operation type: %s", op.description.Type)
}

// ValidateV2 checks if the operation can be performed using ExecutionContext.
func (op *BaseOperation) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	// Convert interfaces back to concrete types
	context, ok := ctx.(context.Context)
	if !ok {
		return fmt.Errorf("invalid context type")
	}

	// For now, delegate to the original method
	// Note: This calls BaseOperation.Validate, not the overridden method!
	// Concrete operations should override ValidateV2 to call their own Validate method.
	return op.Validate(context, fsys)
}
