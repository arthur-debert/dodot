package status

import (
	"fmt"
	iosfs "io/fs"
	"os"
	"path/filepath"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// PathChecker checks the status of PATH operations
type PathChecker struct{}

// NewPathChecker creates a new PATH status checker
func NewPathChecker() *PathChecker {
	return &PathChecker{}
}

// CheckStatus checks if a directory is in PATH and properly deployed
func (pc *PathChecker) CheckStatus(op *types.Operation, fs filesystem.FullFileSystem) (*types.FileStatus, error) {
	status := &types.FileStatus{
		Path:        op.Target,
		PowerUp:     "add_path",
		Status:      types.StatusReady,
		Message:     "Directory not in PATH",
		LastApplied: time.Time{},
		Metadata:    make(map[string]interface{}),
	}

	// For add_path, operations create symlinks in the deployed/path directory
	// Check if the symlink exists
	info, err := fs.Stat(op.Target)
	if err != nil {
		if isNotExist(err) {
			// Symlink doesn't exist in deployment directory
			status.Status = types.StatusReady
			status.Message = "Directory not deployed to PATH"
			status.Metadata["in_path"] = false
			return status, nil
		}
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to check PATH symlink: %v", err)
		return status, nil
	}
	if err != nil {
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to stat PATH symlink: %v", err)
		return status, nil
	}

	if info.Mode()&iosfs.ModeSymlink == 0 {
		// Path exists but is not a symlink
		status.Status = types.StatusConflict
		status.Message = "Deployment path exists but is not a symlink"
		return status, nil
	}

	// Check if symlink points to the correct source
	actualTarget, err := fs.Readlink(op.Target)
	if err != nil {
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to read symlink target: %v", err)
		return status, nil
	}

	// Compare targets - handle both absolute and relative paths
	// The filesystem might return relative paths even when absolute paths were used
	sourceAbs := op.Source
	targetAbs := actualTarget

	// If actualTarget is relative and op.Source is absolute, make actualTarget absolute
	if !filepath.IsAbs(actualTarget) && filepath.IsAbs(op.Source) {
		targetAbs = filepath.Join("/", actualTarget)
	}

	status.Metadata["source_directory"] = op.Source
	status.Metadata["deployed_symlink"] = op.Target
	status.Metadata["actual_target"] = targetAbs

	if targetAbs == sourceAbs {
		// Symlink exists and points to correct directory
		status.Status = types.StatusSkipped
		status.Message = "Directory already deployed to PATH"
		status.LastApplied = info.ModTime()

		// Check if the source directory exists
		_, sourceErr := fs.Stat(actualTarget)
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
