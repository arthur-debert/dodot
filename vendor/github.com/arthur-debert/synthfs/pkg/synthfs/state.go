package synthfs

import (
	"github.com/arthur-debert/synthfs/pkg/synthfs/execution"
)

// PathState is a type alias for execution.PathState
type PathState = execution.PathState

// PathStateTracker is a wrapper around execution.PathStateTracker
type PathStateTracker struct {
	tracker *execution.PathStateTracker
}

// NewPathStateTracker creates a new tracker.
func NewPathStateTracker(fs FullFileSystem) *PathStateTracker {
	return &PathStateTracker{
		tracker: execution.NewPathStateTracker(fs),
	}
}

// GetState returns the projected state of a path.
func (pst *PathStateTracker) GetState(path string) (*PathState, error) {
	return pst.tracker.GetState(path)
}

// UpdateState applies the effect of an operation to the projected state of a path.
func (pst *PathStateTracker) UpdateState(op Operation) error {
	// Create a wrapper that implements execution.OperationInterface
	wrapper := &operationWrapper{op: op}
	return pst.tracker.UpdateState(wrapper)
}

// IsDeleted returns true if the path is scheduled for deletion by any operation.
func (pst *PathStateTracker) IsDeleted(path string) bool {
	return pst.tracker.IsDeleted(path)
}
