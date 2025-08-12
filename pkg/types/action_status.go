package types

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"path/filepath"
	"strings"
	"time"
)

// checkSymlinkStatus checks if a symlink action has been deployed
func (a *Action) checkSymlinkStatus(fs FS, paths Pather) (Status, error) {
	// Get standardized deployed symlink path
	intermediatePath, err := a.GetDeployedSymlinkPath(paths)
	if err != nil {
		return Status{}, err
	}

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
	// For generic run commands, we don't track them
	if a.Type == ActionTypeRun {
		return Status{
			State:   StatusStatePending,
			Message: "will execute script",
		}, nil
	}

	// Get standardized sentinel info
	sentinelInfo, err := a.GetSentinelInfo(paths)
	if err != nil {
		return Status{}, err
	}

	// Check if sentinel exists
	sentinelData, err := fs.ReadFile(sentinelInfo.Path)
	if err != nil {
		// Sentinel doesn't exist - not executed yet
		return Status{
			State:   StatusStatePending,
			Message: "will execute install script",
		}, nil
	}

	// Sentinel exists - parse it to check for modifications
	checksum, timestamp := parseSentinelData(string(sentinelData))

	// Check if source file still exists and hasn't been modified
	sourceData, err := fs.ReadFile(a.Source)
	if err != nil {
		// Source file deleted - still consider it success but note it
		return Status{
			State:   StatusStateSuccess,
			Message: "executed during installation (source file removed)",
		}, nil
	}

	// Calculate current checksum
	currentChecksum := calculateChecksum(sourceData)

	// Compare checksums
	if checksum != "" && checksum != currentChecksum {
		// Script has been modified since execution
		return Status{
			State:     StatusStateError,
			Message:   fmt.Sprintf("executed on %s (source file modified)", timestamp),
			Timestamp: parseTimestamp(timestamp),
		}, nil
	}

	// Script unchanged - successful execution
	return Status{
		State:     StatusStateSuccess,
		Message:   "executed during installation",
		Timestamp: parseTimestamp(timestamp),
	}, nil
}

// checkBrewStatus checks if a Brewfile has been processed
func (a *Action) checkBrewStatus(fs FS, paths Pather) (Status, error) {
	// Get standardized sentinel info
	sentinelInfo, err := a.GetSentinelInfo(paths)
	if err != nil {
		return Status{}, err
	}

	// Check if sentinel exists
	sentinelData, err := fs.ReadFile(sentinelInfo.Path)
	if err != nil {
		// Sentinel doesn't exist - not executed yet
		return Status{
			State:   StatusStatePending,
			Message: "will run homebrew install",
		}, nil
	}

	// Sentinel exists - parse it to check for modifications
	checksum, timestamp := parseSentinelData(string(sentinelData))

	// Check if Brewfile still exists and hasn't been modified
	sourceData, err := fs.ReadFile(a.Source)
	if err != nil {
		// Brewfile deleted - still consider it success but note it
		return Status{
			State:   StatusStateSuccess,
			Message: "homebrew packages installed (Brewfile removed)",
		}, nil
	}

	// Calculate current checksum
	currentChecksum := calculateChecksum(sourceData)

	// Compare checksums
	if checksum != "" && checksum != currentChecksum {
		// Brewfile has been modified since execution
		return Status{
			State:     StatusStateError,
			Message:   fmt.Sprintf("executed on %s (Brewfile modified)", timestamp),
			Timestamp: parseTimestamp(timestamp),
		}, nil
	}

	// Brewfile unchanged - successful execution
	return Status{
		State:     StatusStateSuccess,
		Message:   "homebrew packages installed",
		Timestamp: parseTimestamp(timestamp),
	}, nil
}

// checkPathStatus checks if a directory has been added to PATH
func (a *Action) checkPathStatus(fs FS, paths Pather) (Status, error) {
	// Get standardized deployed path
	linkPath, err := a.GetDeployedPathPath(paths)
	if err != nil {
		return Status{}, err
	}

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
	// Get standardized deployed shell profile path
	linkPath, err := a.GetDeployedShellProfilePath(paths)
	if err != nil {
		return Status{}, err
	}

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

// Helper functions

// parseSentinelData parses the sentinel file content
// Expected format: "checksum:timestamp" or just "timestamp" for legacy sentinels
func parseSentinelData(data string) (checksum, timestamp string) {
	parts := strings.SplitN(strings.TrimSpace(data), ":", 2)
	if len(parts) == 2 {
		return parts[0], parts[1]
	}
	// Legacy format with just timestamp
	return "", parts[0]
}

// calculateChecksum calculates SHA256 checksum of data
func calculateChecksum(data []byte) string {
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:])
}

// parseTimestamp parses a timestamp string and returns a time pointer
func parseTimestamp(timestamp string) *time.Time {
	if timestamp == "" {
		return nil
	}

	// Try parsing common formats
	formats := []string{
		time.RFC3339,
		"2006-01-02T15:04:05Z07:00",
		"2006-01-02 15:04:05",
		"2006-01-02",
	}

	for _, format := range formats {
		if t, err := time.Parse(format, timestamp); err == nil {
			return &t
		}
	}

	return nil
}
