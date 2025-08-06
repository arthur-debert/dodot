package status

import (
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// SymlinkChecker checks the status of symlink operations
type SymlinkChecker struct {
	*BaseChecker
}

// NewSymlinkChecker creates a new symlink status checker
func NewSymlinkChecker() *SymlinkChecker {
	return &SymlinkChecker{
		BaseChecker: &BaseChecker{
			PowerUpName: "symlink",
		},
	}
}

// CheckStatus checks if a symlink exists and points to the correct target
func (sc *SymlinkChecker) CheckStatus(op *types.Operation, fs filesystem.FullFileSystem) (*types.FileStatus, error) {
	status := sc.InitializeStatus(op.Target, "Symlink not created")

	// Store the expected target in metadata
	status.Metadata["expected_target"] = op.Source

	// Check symlink status
	result, err := sc.CheckSymlink(fs, op.Target)
	if err != nil {
		sc.SetError(status, "check symlink", err)
		return status, nil
	}

	// Handle non-existent symlink
	if !result.Exists {
		status.Status = types.StatusReady
		status.Message = "Symlink does not exist"
		return status, nil
	}

	// Handle existing file that's not a symlink
	if !result.IsSymlink {
		status.Status = types.StatusConflict
		status.Message = "Path exists but is not a symlink"
		// Add file type metadata
		info, _ := fs.Stat(op.Target)
		if info != nil {
			status.Metadata["is_directory"] = info.IsDir()
			status.Metadata["is_regular_file"] = info.Mode().IsRegular()
		}
		return status, nil
	}

	// Set symlink metadata
	sc.SetSymlinkMetadata(status, result, op.Source)

	// Compare targets
	if sc.CompareSymlinkTargets(op.Source, result.ActualTarget) {
		// Symlink exists and points to the correct target
		status.Status = types.StatusSkipped
		status.Message = "Symlink already exists with correct target"
		status.Metadata["link_valid"] = true
	} else {
		// Symlink exists but points to wrong target
		status.Status = types.StatusConflict
		status.Message = "Symlink exists but points to wrong target"
		status.Metadata["link_valid"] = false
	}

	// Check if the target exists
	_, targetErr := fs.Stat(result.ActualTarget)
	status.Metadata["target_exists"] = targetErr == nil

	return status, nil
}
