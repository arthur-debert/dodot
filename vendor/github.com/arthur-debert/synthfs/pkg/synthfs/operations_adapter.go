package synthfs

import (
	"context"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
	"github.com/arthur-debert/synthfs/pkg/synthfs/operations"
)

// OperationsPackageAdapter adapts an operations.Operation to implement the main package Operation interface.
// This allows gradual migration from SimpleOperation to the operations package.
type OperationsPackageAdapter struct {
	opsOperation operations.Operation
}

// NewOperationsPackageAdapter creates a new adapter for an operations package operation.
func NewOperationsPackageAdapter(opsOp operations.Operation) Operation {
	return &OperationsPackageAdapter{
		opsOperation: opsOp,
	}
}

// ID returns the operation's ID.
func (a *OperationsPackageAdapter) ID() core.OperationID {
	return a.opsOperation.ID()
}

// Describe returns the operation's description.
func (a *OperationsPackageAdapter) Describe() core.OperationDesc {
	return a.opsOperation.Describe()
}

// Dependencies returns the operation's dependencies.
func (a *OperationsPackageAdapter) Dependencies() []core.OperationID {
	return a.opsOperation.Dependencies()
}

// Conflicts returns the operation's conflicts.
func (a *OperationsPackageAdapter) Conflicts() []core.OperationID {
	return a.opsOperation.Conflicts()
}

// Prerequisites returns the operation's prerequisites.
func (a *OperationsPackageAdapter) Prerequisites() []core.Prerequisite {
	return a.opsOperation.Prerequisites()
}

// Execute performs the operation.
func (a *OperationsPackageAdapter) Execute(ctx context.Context, fsys FileSystem) error {
	return a.opsOperation.Execute(ctx, fsys)
}

// Validate checks if the operation can be performed.
func (a *OperationsPackageAdapter) Validate(ctx context.Context, fsys FileSystem) error {
	return a.opsOperation.Validate(ctx, fsys)
}

// ExecuteV2 performs the operation using ExecutionContext.
func (a *OperationsPackageAdapter) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return a.opsOperation.ExecuteV2(ctx, execCtx, fsys)
}

// ValidateV2 checks if the operation can be performed using ExecutionContext.
func (a *OperationsPackageAdapter) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return a.opsOperation.ValidateV2(ctx, execCtx, fsys)
}

// Rollback undoes the operation.
func (a *OperationsPackageAdapter) Rollback(ctx context.Context, fsys FileSystem) error {
	return a.opsOperation.Rollback(ctx, fsys)
}

// GetItem returns the item associated with this operation.
func (a *OperationsPackageAdapter) GetItem() FsItem {
	item := a.opsOperation.GetItem()
	if item == nil {
		return nil
	}

	// Try to convert interface{} to FsItem
	if fsItem, ok := item.(FsItem); ok {
		return fsItem
	}

	// If it's from the operations package, we might need to adapt it
	// For now, return nil if we can't convert
	return nil
}

// GetChecksum returns a checksum for the given path.
func (a *OperationsPackageAdapter) GetChecksum(path string) *ChecksumRecord {
	cs := a.opsOperation.GetChecksum(path)
	if cs == nil {
		return nil
	}

	// Try to convert interface{} to ChecksumRecord
	if checksum, ok := cs.(*ChecksumRecord); ok {
		return checksum
	}

	// If it's a different type, try to adapt it
	// For now, return nil if we can't convert
	return nil
}

// GetAllChecksums returns all checksums.
func (a *OperationsPackageAdapter) GetAllChecksums() map[string]*ChecksumRecord {
	checksums := a.opsOperation.GetAllChecksums()
	if checksums == nil {
		return nil
	}

	result := make(map[string]*ChecksumRecord)
	for path, cs := range checksums {
		if checksum, ok := cs.(*ChecksumRecord); ok {
			result[path] = checksum
		}
	}

	return result
}

// ReverseOps generates operations to reverse this operation.
func (a *OperationsPackageAdapter) ReverseOps(ctx context.Context, fsys FileSystem, budget *core.BackupBudget) ([]Operation, *core.BackupData, error) {
	ops, data, err := a.opsOperation.ReverseOps(ctx, fsys, budget)

	// Convert operations
	var result []Operation
	for _, op := range ops {
		if opsOp, ok := op.(operations.Operation); ok {
			result = append(result, NewOperationsPackageAdapter(opsOp))
		} else if mainOp, ok := op.(Operation); ok {
			result = append(result, mainOp)
		}
	}

	// Convert backup data
	var backupData *core.BackupData
	if data != nil {
		if bd, ok := data.(*core.BackupData); ok {
			backupData = bd
		}
	} else if err == nil && len(result) > 0 {
		// If no backup data but operation succeeded, create a "none" type backup data
		// This maintains compatibility with synthfs package expectations
		backupData = &core.BackupData{
			OperationID: a.opsOperation.ID(),
			BackupType:  "none",
			BackupTime:  time.Now(),
			SizeMB:      0,
		}
	}

	// Return the backup data even if there was an error
	return result, backupData, err
}

// SetDescriptionDetail sets a detail in the operation's description.
func (a *OperationsPackageAdapter) SetDescriptionDetail(key string, value interface{}) {
	a.opsOperation.SetDescriptionDetail(key, value)
}

// AddDependency adds a dependency to the operation.
func (a *OperationsPackageAdapter) AddDependency(depID core.OperationID) {
	a.opsOperation.AddDependency(depID)
}

// SetPaths sets the source and destination paths.
func (a *OperationsPackageAdapter) SetPaths(src, dst string) {
	a.opsOperation.SetPaths(src, dst)
}

// SetChecksum sets a checksum for the given path.
func (a *OperationsPackageAdapter) SetChecksum(path string, checksum *ChecksumRecord) {
	// The operations package uses interface{}, so we can pass ChecksumRecord directly
	a.opsOperation.SetChecksum(path, checksum)
}
