package status

import (
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// ProfileChecker checks the status of shell profile operations
type ProfileChecker struct {
	BaseChecker
}

// NewProfileChecker creates a new profile status checker
func NewProfileChecker() *ProfileChecker {
	return &ProfileChecker{
		BaseChecker: BaseChecker{
			PowerUpName: "shell_profile",
		},
	}
}

// CheckStatus checks if a shell profile entry exists
func (pc *ProfileChecker) CheckStatus(op *types.Operation, fs filesystem.FullFileSystem) (*types.FileStatus, error) {
	status := pc.InitializeStatus(op.Target, "Profile entry not configured")

	// For shell_profile, operations create symlinks in the deployment directory
	// Check if the symlink exists
	result, err := pc.CheckSymlink(fs, op.Target)
	if err != nil {
		pc.SetError(status, "check profile symlink", err)
		return status, nil
	}

	if !result.Exists {
		// Symlink doesn't exist in deployment directory
		status.Status = types.StatusReady
		status.Message = "Profile script not deployed"
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
	status.Metadata["source_script"] = op.Source
	status.Metadata["deployed_symlink"] = op.Target

	// Override actual_target with the processed version for backward compatibility
	actualAbs := result.ActualTarget
	if !filepath.IsAbs(result.ActualTarget) && filepath.IsAbs(op.Source) {
		actualAbs = filepath.Join("/", result.ActualTarget)
	}
	status.Metadata["actual_target"] = actualAbs

	// Compare targets
	if pc.CompareSymlinkTargets(op.Source, result.ActualTarget) {
		// Symlink exists and points to correct script
		status.Status = types.StatusSkipped
		status.Message = "Profile script already deployed"

		// Check if the source script exists
		_, sourceErr := fs.Stat(result.ActualTarget)
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
