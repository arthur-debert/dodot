package status

import (
	"fmt"
	iosfs "io/fs"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// SymlinkChecker checks the status of symlink operations
type SymlinkChecker struct{}

// NewSymlinkChecker creates a new symlink status checker
func NewSymlinkChecker() *SymlinkChecker {
	return &SymlinkChecker{}
}

// CheckStatus checks if a symlink exists and points to the correct target
func (sc *SymlinkChecker) CheckStatus(op *types.Operation, fs filesystem.FullFileSystem) (*types.FileStatus, error) {
	status := &types.FileStatus{
		Path:        op.Target,
		PowerUp:     "symlink",
		Status:      types.StatusReady,
		Message:     "Symlink not created",
		LastApplied: time.Time{},
		Metadata:    make(map[string]interface{}),
	}

	// Store the expected target in metadata
	status.Metadata["expected_target"] = op.Source

	// Check if the symlink exists
	info, err := fs.Stat(op.Target)
	if err != nil {
		if isNotExist(err) {
			// Symlink doesn't exist, it's ready to be created
			status.Status = types.StatusReady
			status.Message = "Symlink does not exist"
			return status, nil
		}
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to check symlink: %v", err)
		return status, nil
	}
	if err != nil {
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to stat symlink: %v", err)
		return status, nil
	}

	if info.Mode()&iosfs.ModeSymlink == 0 {
		// Path exists but is not a symlink
		status.Status = types.StatusConflict
		status.Message = "Path exists but is not a symlink"
		status.Metadata["is_directory"] = info.IsDir()
		status.Metadata["is_regular_file"] = info.Mode().IsRegular()
		return status, nil
	}

	// It's a symlink, check if it points to the correct target
	actualTarget, err := fs.Readlink(op.Target)
	if err != nil {
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to read symlink target: %v", err)
		return status, nil
	}

	status.Metadata["actual_target"] = actualTarget

	// Compare targets
	if actualTarget == op.Source {
		// Symlink exists and points to the correct target
		status.Status = types.StatusSkipped
		status.Message = "Symlink already exists with correct target"
		status.LastApplied = info.ModTime()
		status.Metadata["link_valid"] = true
	} else {
		// Symlink exists but points to wrong target
		status.Status = types.StatusConflict
		status.Message = "Symlink exists but points to wrong target"
		status.Metadata["link_valid"] = false
	}

	// Check if the target exists
	_, targetErr := fs.Stat(actualTarget)
	status.Metadata["target_exists"] = targetErr == nil

	return status, nil
}
