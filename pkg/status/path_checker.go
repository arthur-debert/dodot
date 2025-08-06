package status

import (
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// PathChecker checks the status of PATH operations
type PathChecker struct {
	BaseChecker
}

// NewPathChecker creates a new PATH status checker
func NewPathChecker() *PathChecker {
	return &PathChecker{
		BaseChecker: BaseChecker{
			PowerUpName: "path",
		},
	}
}

// CheckStatus checks if a directory is in PATH and properly deployed
func (pc *PathChecker) CheckStatus(op *types.Operation, fs filesystem.FullFileSystem) (*types.FileStatus, error) {
	status := pc.InitializeStatus(op.Target, "Directory not in PATH")
	// Override the PowerUp name to match the existing behavior
	status.PowerUp = "add_path"

	// For add_path, operations create symlinks in the deployed/path directory
	// Check if the symlink exists
	result, err := pc.CheckSymlink(fs, op.Target)
	if err != nil {
		pc.SetError(status, "check PATH symlink", err)
		return status, nil
	}

	if !result.Exists {
		// Symlink doesn't exist in deployment directory
		status.Status = types.StatusReady
		status.Message = "Directory not deployed to PATH"
		status.Metadata["in_path"] = false
		return status, nil
	}

	if !result.IsSymlink {
		// Path exists but is not a symlink
		status.Status = types.StatusConflict
		status.Message = "Deployment path exists but is not a symlink"
		return status, nil
	}

	// Set symlink metadata
	pc.SetSymlinkMetadata(status, result, op.Source)
	status.Metadata["source_directory"] = op.Source
	status.Metadata["deployed_symlink"] = op.Target

	// Override actual_target with the processed version for backward compatibility
	actualAbs := result.ActualTarget
	if !filepath.IsAbs(result.ActualTarget) && filepath.IsAbs(op.Source) {
		actualAbs = filepath.Join("/", result.ActualTarget)
	}
	status.Metadata["actual_target"] = actualAbs

	// Compare targets
	if pc.CompareSymlinkTargets(op.Source, result.ActualTarget) {
		// Symlink exists and points to correct directory
		status.Status = types.StatusSkipped
		status.Message = "Directory already deployed to PATH"

		// Check if the source directory exists
		_, sourceErr := fs.Stat(result.ActualTarget)
		status.Metadata["source_exists"] = sourceErr == nil

		// Check if directory is actually in current PATH
		currentPath := os.Getenv("PATH")
		inPath := pc.isDirectoryInPath(op.Target, currentPath)
		status.Metadata["in_current_path"] = inPath

		if !inPath {
			status.Message = "Directory deployed but not in current PATH (shell restart may be needed)"
		}
	} else {
		// Symlink exists but points to wrong target
		status.Status = types.StatusConflict
		status.Message = "PATH symlink points to wrong directory"
	}

	return status, nil
}

// isDirectoryInPath checks if a directory is in the PATH environment variable
func (pc *PathChecker) isDirectoryInPath(dir string, pathEnv string) bool {
	// If the directory is relative, don't try to match it
	// Relative paths in PATH are context-dependent and unreliable
	if !filepath.IsAbs(dir) {
		return false
	}

	// Resolve to absolute path for comparison
	absDir, err := filepath.Abs(dir)
	if err != nil {
		return false
	}

	// Split PATH and check each entry
	paths := filepath.SplitList(pathEnv)
	for _, p := range paths {
		// Skip relative paths in PATH - they're unreliable
		if !filepath.IsAbs(p) {
			continue
		}

		absPath, err := filepath.Abs(p)
		if err != nil {
			continue
		}
		if absPath == absDir {
			return true
		}
	}

	return false
}
