package types

import (
	"fmt"
	"path/filepath"
	"strings"
)

// checkSymlinkStatus checks if a symlink action has been deployed
func (a *Action) checkSymlinkStatus(fs FS, paths Pather) (Status, error) {
	// Check intermediate symlink in deployed/symlink/
	intermediatePath := filepath.Join(paths.DataDir(), "deployed", "symlink", filepath.Base(a.Target))

	if _, err := fs.Lstat(intermediatePath); err == nil {
		// Intermediate symlink exists, check if source still exists
		if _, err := fs.Stat(a.Source); err != nil {
			return Status{
				State:   StatusStateError,
				Message: fmt.Sprintf("linked to %s (broken - source file missing)", filepath.Base(a.Target)),
			}, nil
		}
		return Status{
			State:   StatusStateSuccess,
			Message: fmt.Sprintf("linked to %s", filepath.Base(a.Target)),
		}, nil
	}

	// Not deployed yet
	return Status{
		State:   StatusStatePending,
		Message: fmt.Sprintf("will symlink to %s", filepath.Base(a.Target)),
	}, nil
}

// checkScriptStatus checks if a script (install/run) action has been executed
func (a *Action) checkScriptStatus(fs FS, paths Pather) (Status, error) {
	// Determine sentinel directory based on action type
	var sentinelDir string
	switch a.Type {
	case ActionTypeInstall:
		sentinelDir = filepath.Join(paths.DataDir(), "install")
	case ActionTypeRun:
		// For generic run commands, we might not track them
		// For now, always return pending
		return Status{
			State:   StatusStatePending,
			Message: "will execute script",
		}, nil
	}

	// Check for sentinel file
	// Sentinel filename is based on pack name and script name
	sentinelName := fmt.Sprintf("%s_%s.sentinel", a.Pack, filepath.Base(a.Source))
	sentinelPath := filepath.Join(sentinelDir, sentinelName)

	if _, err := fs.Stat(sentinelPath); err == nil {
		return Status{
			State:   StatusStateSuccess,
			Message: "executed during installation",
		}, nil
	}

	return Status{
		State:   StatusStatePending,
		Message: "will execute install script",
	}, nil
}

// checkBrewStatus checks if a Brewfile has been processed
func (a *Action) checkBrewStatus(fs FS, paths Pather) (Status, error) {
	// Check for Brewfile sentinel
	sentinelDir := filepath.Join(paths.DataDir(), "homebrew")
	sentinelName := fmt.Sprintf("%s_Brewfile.sentinel", a.Pack)
	sentinelPath := filepath.Join(sentinelDir, sentinelName)

	if _, err := fs.Stat(sentinelPath); err == nil {
		return Status{
			State:   StatusStateSuccess,
			Message: "homebrew packages installed",
		}, nil
	}

	return Status{
		State:   StatusStatePending,
		Message: "will run homebrew install",
	}, nil
}

// checkPathStatus checks if a directory has been added to PATH
func (a *Action) checkPathStatus(fs FS, paths Pather) (Status, error) {
	// Check if path symlink exists in deployed/path/
	// The symlink name is typically the pack name or directory name
	pathDir := filepath.Join(paths.DataDir(), "deployed", "path")

	// Try to find a matching symlink in the path directory
	// This is a simplified check - real implementation might need more logic
	linkName := filepath.Base(a.Source)
	if a.Pack != "" {
		linkName = a.Pack + "_" + linkName
	}
	linkPath := filepath.Join(pathDir, linkName)

	if _, err := fs.Lstat(linkPath); err == nil {
		return Status{
			State:   StatusStateSuccess,
			Message: "added to PATH",
		}, nil
	}

	return Status{
		State:   StatusStatePending,
		Message: "will add to PATH",
	}, nil
}

// checkShellSourceStatus checks if a shell script is being sourced
func (a *Action) checkShellSourceStatus(fs FS, paths Pather) (Status, error) {
	// Check if script symlink exists in deployed/shell_profile/
	profileDir := filepath.Join(paths.DataDir(), "deployed", "shell_profile")

	// The symlink name includes pack name for uniqueness
	linkName := fmt.Sprintf("%s_%s.sh", a.Pack, strings.TrimSuffix(filepath.Base(a.Source), filepath.Ext(a.Source)))
	linkPath := filepath.Join(profileDir, linkName)

	if _, err := fs.Lstat(linkPath); err == nil {
		// Get shell type from metadata if available
		shellType := "shell"
		if shell, ok := a.Metadata["shell"].(string); ok {
			shellType = shell
		}
		return Status{
			State:   StatusStateSuccess,
			Message: fmt.Sprintf("sourced in %s", shellType),
		}, nil
	}

	return Status{
		State:   StatusStatePending,
		Message: "will be sourced in shell init",
	}, nil
}

// checkWriteStatus checks if a file has been written
func (a *Action) checkWriteStatus(fs FS, paths Pather) (Status, error) {
	// For write/append actions, just check if target file exists
	if _, err := fs.Stat(a.Target); err == nil {
		if a.Type == ActionTypeAppend {
			return Status{
				State:   StatusStateSuccess,
				Message: "content appended",
			}, nil
		}
		return Status{
			State:   StatusStateSuccess,
			Message: "file created",
		}, nil
	}

	if a.Type == ActionTypeAppend {
		return Status{
			State:   StatusStatePending,
			Message: "will append content",
		}, nil
	}
	return Status{
		State:   StatusStatePending,
		Message: "will create file",
	}, nil
}

// checkMkdirStatus checks if a directory has been created
func (a *Action) checkMkdirStatus(fs FS, paths Pather) (Status, error) {
	// Check if directory exists
	if info, err := fs.Stat(a.Target); err == nil && info.IsDir() {
		return Status{
			State:   StatusStateSuccess,
			Message: "directory created",
		}, nil
	}

	return Status{
		State:   StatusStatePending,
		Message: "will create directory",
	}, nil
}
