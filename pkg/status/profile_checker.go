package status

import (
	"fmt"
	iosfs "io/fs"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// ProfileChecker checks the status of shell profile operations
type ProfileChecker struct{}

// NewProfileChecker creates a new profile status checker
func NewProfileChecker() *ProfileChecker {
	return &ProfileChecker{}
}

// CheckStatus checks if a shell profile entry exists
func (pc *ProfileChecker) CheckStatus(op *types.Operation, fs filesystem.FullFileSystem) (*types.FileStatus, error) {
	status := &types.FileStatus{
		Path:        op.Target,
		PowerUp:     "shell_profile",
		Status:      types.StatusReady,
		Message:     "Profile entry not configured",
		LastApplied: time.Time{},
		Metadata:    make(map[string]interface{}),
	}

	// For shell_profile, operations create symlinks in the deployment directory
	// Check if the symlink exists
	info, err := fs.Stat(op.Target)
	if err != nil {
		if isNotExist(err) {
			// Symlink doesn't exist in deployment directory
			status.Status = types.StatusReady
			status.Message = "Profile script not deployed"
			return status, nil
		}
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to check profile symlink: %v", err)
		return status, nil
	}
	if err != nil {
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to stat profile symlink: %v", err)
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

	status.Metadata["source_script"] = op.Source
	status.Metadata["deployed_symlink"] = op.Target
	status.Metadata["actual_target"] = actualTarget

	// Compare targets
	if actualTarget == op.Source {
		// Symlink exists and points to correct script
		status.Status = types.StatusSkipped
		status.Message = "Profile script already deployed"
		status.LastApplied = info.ModTime()

		// Check if the source script exists
		_, sourceErr := fs.Stat(actualTarget)
		status.Metadata["source_exists"] = sourceErr == nil

		// Add which shell files would source this script
		status.Metadata["loaded_by"] = pc.getLoadingShells(op.Target)
	} else {
		// Symlink exists but points to wrong target
		status.Status = types.StatusConflict
		status.Message = "Profile symlink points to wrong script"
	}

	return status, nil
}

// getLoadingShells returns a list of shell configurations that would load this profile script
func (pc *ProfileChecker) getLoadingShells(deployedPath string) []string {
	// Profile scripts in deployed/shell_profile/ are sourced by dodot-init.sh
	// which is typically sourced by .bashrc, .bash_profile, .zshrc, etc.
	shells := []string{}

	// Extract the deployment directory structure
	if strings.Contains(deployedPath, "/deployed/shell_profile/") {
		// This script would be loaded by any shell that sources dodot-init.sh
		shells = append(shells, "bash (via dodot-init.sh)", "zsh (via dodot-init.sh)")
	}

	if len(shells) == 0 {
		shells = append(shells, "none (dodot-init.sh not sourced)")
	}

	return shells
}
