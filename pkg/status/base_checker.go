package status

import (
	"fmt"
	iosfs "io/fs"
	"path/filepath"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// BaseChecker provides common functionality for all status checkers
type BaseChecker struct {
	PowerUpName string
}

// InitializeStatus creates a new FileStatus with default values
func (bc *BaseChecker) InitializeStatus(path string, defaultMessage string) *types.FileStatus {
	return &types.FileStatus{
		Path:        path,
		PowerUp:     bc.PowerUpName,
		Status:      types.StatusReady,
		Message:     defaultMessage,
		LastApplied: time.Time{},
		Metadata:    make(map[string]interface{}),
	}
}

// HandleStatError handles common fs.Stat error cases
func (bc *BaseChecker) HandleStatError(status *types.FileStatus, err error, notExistMessage string) (*types.FileStatus, error) {
	if isNotExist(err) {
		status.Status = types.StatusReady
		status.Message = notExistMessage
		return status, nil
	}
	status.Status = types.StatusError
	status.Message = fmt.Sprintf("Failed to check %s: %v", bc.PowerUpName, err)
	return status, nil
}

// SetError sets the status to error with a formatted message
func (bc *BaseChecker) SetError(status *types.FileStatus, action string, err error) {
	status.Status = types.StatusError
	status.Message = fmt.Sprintf("Failed to %s: %v", action, err)
}

// SymlinkCheckResult contains the result of checking a symlink
type SymlinkCheckResult struct {
	Exists       bool
	IsSymlink    bool
	ActualTarget string
	ModTime      time.Time
}

// CheckSymlink checks if a path is a valid symlink and returns its properties
func (bc *BaseChecker) CheckSymlink(fs filesystem.FullFileSystem, path string) (*SymlinkCheckResult, error) {
	info, err := fs.Stat(path)
	if err != nil {
		if isNotExist(err) {
			return &SymlinkCheckResult{Exists: false}, nil
		}
		return nil, err
	}

	result := &SymlinkCheckResult{
		Exists:  true,
		ModTime: info.ModTime(),
	}

	// Check if it's a symlink
	if info.Mode()&iosfs.ModeSymlink == 0 {
		result.IsSymlink = false
		return result, nil
	}

	// Read the symlink target
	actualTarget, err := fs.Readlink(path)
	if err != nil {
		return nil, fmt.Errorf("read symlink: %w", err)
	}

	result.IsSymlink = true
	result.ActualTarget = actualTarget
	return result, nil
}

// CompareSymlinkTargets compares expected and actual symlink targets,
// handling both absolute and relative paths
func (bc *BaseChecker) CompareSymlinkTargets(expected, actual string) bool {
	// Direct comparison first
	if expected == actual {
		return true
	}

	// If actual is relative and expected is absolute, make actual absolute
	expectedAbs := expected
	actualAbs := actual

	if !filepath.IsAbs(actual) && filepath.IsAbs(expected) {
		actualAbs = filepath.Join("/", actual)
	}

	return expectedAbs == actualAbs
}

// SetSymlinkMetadata sets common symlink-related metadata
func (bc *BaseChecker) SetSymlinkMetadata(status *types.FileStatus, result *SymlinkCheckResult, expectedTarget string) {
	if result.Exists {
		status.LastApplied = result.ModTime
		if result.IsSymlink {
			status.Metadata["actual_target"] = result.ActualTarget
			status.Metadata["expected_target"] = expectedTarget
			status.Metadata["is_symlink"] = true
		} else {
			status.Metadata["is_symlink"] = false
		}
	}
}
