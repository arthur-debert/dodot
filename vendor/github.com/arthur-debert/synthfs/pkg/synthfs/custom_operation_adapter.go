package synthfs

import (
	"context"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// CustomOperationAdapter adapts CustomOperation to implement the synthfs.Operation interface
type CustomOperationAdapter struct {
	*CustomOperation
}

// NewCustomOperationAdapter creates a new adapter for a custom operation
func NewCustomOperationAdapter(op *CustomOperation) *CustomOperationAdapter {
	return &CustomOperationAdapter{CustomOperation: op}
}

// These methods delegate to the embedded CustomOperation

func (a *CustomOperationAdapter) ID() OperationID {
	return OperationID(a.CustomOperation.ID())
}

func (a *CustomOperationAdapter) Describe() OperationDesc {
	desc := a.CustomOperation.Describe()
	return OperationDesc(desc)
}

func (a *CustomOperationAdapter) Dependencies() []OperationID {
	coreDeps := a.CustomOperation.Dependencies()
	deps := make([]OperationID, len(coreDeps))
	for i, dep := range coreDeps {
		deps[i] = OperationID(dep)
	}
	return deps
}

func (a *CustomOperationAdapter) Conflicts() []OperationID {
	coreConflicts := a.CustomOperation.Conflicts()
	conflicts := make([]OperationID, len(coreConflicts))
	for i, conflict := range coreConflicts {
		conflicts[i] = OperationID(conflict)
	}
	return conflicts
}

func (a *CustomOperationAdapter) Prerequisites() []core.Prerequisite {
	return a.CustomOperation.Prerequisites()
}

func (a *CustomOperationAdapter) Execute(ctx context.Context, fsys FileSystem) error {
	return a.CustomOperation.Execute(ctx, fsys)
}

func (a *CustomOperationAdapter) Validate(ctx context.Context, fsys FileSystem) error {
	return a.CustomOperation.Validate(ctx, fsys)
}

func (a *CustomOperationAdapter) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return a.CustomOperation.ExecuteV2(ctx, execCtx, fsys)
}

func (a *CustomOperationAdapter) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return a.CustomOperation.ValidateV2(ctx, execCtx, fsys)
}

func (a *CustomOperationAdapter) Rollback(ctx context.Context, fsys FileSystem) error {
	return a.CustomOperation.Rollback(ctx, fsys)
}

func (a *CustomOperationAdapter) GetItem() FsItem {
	// Custom operations don't have filesystem items
	return nil
}

func (a *CustomOperationAdapter) GetChecksum(path string) *ChecksumRecord {
	// Custom operations don't manage checksums
	return nil
}

func (a *CustomOperationAdapter) GetAllChecksums() map[string]*ChecksumRecord {
	// Custom operations don't manage checksums
	return nil
}

func (a *CustomOperationAdapter) ReverseOps(ctx context.Context, fsys FileSystem, budget *BackupBudget) ([]Operation, *BackupData, error) {
	// Get reverse operations from CustomOperation
	reverseOps, backupData, err := a.CustomOperation.ReverseOps(ctx, fsys, budget)
	if err != nil {
		return nil, nil, err
	}

	// Convert reverse operations to synthfs.Operation
	var ops []Operation
	for _, op := range reverseOps {
		if customOp, ok := op.(*CustomOperation); ok {
			ops = append(ops, NewCustomOperationAdapter(customOp))
		}
	}

	// Convert backup data if present
	var bd *BackupData
	if backupData != nil {
		if data, ok := backupData.(*core.BackupData); ok {
			bd = (*BackupData)(data)
		}
	}

	return ops, bd, nil
}

func (a *CustomOperationAdapter) SetDescriptionDetail(key string, value interface{}) {
	a.CustomOperation.SetDescriptionDetail(key, value)
}

func (a *CustomOperationAdapter) AddDependency(depID OperationID) {
	a.CustomOperation.AddDependency(core.OperationID(depID))
}

func (a *CustomOperationAdapter) SetPaths(src, dst string) {
	a.CustomOperation.SetPaths(src, dst)
}

// Ensure CustomOperationAdapter implements the Operation interface
var _ Operation = (*CustomOperationAdapter)(nil)
