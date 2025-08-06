package status

import (
	"fmt"
	"io"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// BrewChecker checks the status of Homebrew operations
type BrewChecker struct{}

// NewBrewChecker creates a new Homebrew status checker
func NewBrewChecker() *BrewChecker {
	return &BrewChecker{}
}

// CheckStatus checks if Homebrew packages are installed and up to date
func (bc *BrewChecker) CheckStatus(op *types.Operation, fs filesystem.FullFileSystem) (*types.FileStatus, error) {
	status := &types.FileStatus{
		Path:        op.Source, // Brewfile path
		PowerUp:     "homebrew",
		Status:      types.StatusReady,
		Message:     "Brewfile not processed",
		LastApplied: time.Time{},
		Metadata:    make(map[string]interface{}),
	}

	// For homebrew, we check:
	// 1. If a sentinel file exists with the checksum
	// 2. If the checksum matches the current Brewfile
	// 3. Optionally check if packages are actually installed

	// Extract pack name from metadata or operation
	pack := ""
	if op.Metadata != nil {
		if p, ok := op.Metadata["pack"].(string); ok {
			pack = p
		}
	}
	if pack == "" {
		// Try to extract from path
		pack = filepath.Base(filepath.Dir(op.Source))
	}

	status.Metadata["pack"] = pack
	status.Metadata["brewfile"] = op.Source

	// Check for sentinel file
	// The sentinel path would be in the data directory: deployed/homebrew/<pack>
	sentinelPath := filepath.Join(filepath.Dir(op.Target), pack)
	if op.Type == types.OperationWriteFile && strings.Contains(op.Target, "/homebrew/") {
		// This is the sentinel file write operation
		sentinelPath = op.Target
	}

	_, err := fs.Stat(sentinelPath)
	if err != nil {
		if isNotExist(err) {
			// No sentinel file, packages need to be installed
			status.Status = types.StatusReady
			status.Message = "Brewfile not processed (no sentinel file)"
			status.Metadata["sentinel_exists"] = false
			return status, nil
		}
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to check sentinel file: %v", err)
		return status, nil
	}

	// Read sentinel file to get stored checksum
	reader, err := fs.Open(sentinelPath)
	if err != nil {
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to read sentinel file: %v", err)
		return status, nil
	}
	defer func() {
		_ = reader.Close()
	}()

	storedChecksumBytes, err := io.ReadAll(reader)
	if err != nil {
		status.Status = types.StatusError
		status.Message = fmt.Sprintf("Failed to read sentinel checksum: %v", err)
		return status, nil
	}

	storedChecksum := strings.TrimSpace(string(storedChecksumBytes))
	status.Metadata["stored_checksum"] = storedChecksum
	status.Metadata["sentinel_exists"] = true

	// Get current checksum from operation content or metadata
	currentChecksum := ""
	if op.Content != "" {
		// This is the sentinel write operation, content is the checksum
		currentChecksum = op.Content
	} else if op.Metadata != nil {
		if cs, ok := op.Metadata["checksum"].(string); ok {
			currentChecksum = cs
		}
	}

	if currentChecksum == "" {
		// Can't determine current checksum
		status.Status = types.StatusUnknown
		status.Message = "Cannot determine current Brewfile checksum"
		return status, nil
	}

	status.Metadata["current_checksum"] = currentChecksum

	// Compare checksums
	if storedChecksum == currentChecksum {
		// Checksums match, packages should be installed
		status.Status = types.StatusSkipped
		status.Message = "Brewfile already processed (checksum matches)"

		// Get sentinel file info for last applied time
		if info, err := fs.Stat(sentinelPath); err == nil {
			status.LastApplied = info.ModTime()
		}

		// Optionally check if brew command is available
		if _, err := exec.LookPath("brew"); err == nil {
			status.Metadata["brew_available"] = true
		} else {
			status.Metadata["brew_available"] = false
			status.Message = "Brewfile processed but brew command not found"
		}
	} else {
		// Checksums don't match, Brewfile has changed
		status.Status = types.StatusReady
		status.Message = "Brewfile has changed (checksum mismatch)"
		status.Metadata["checksum_match"] = false
	}

	return status, nil
}
