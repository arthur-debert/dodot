package status

import (
	"os/exec"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// BrewChecker checks the status of Homebrew operations
type BrewChecker struct {
	*BaseChecker
	*SentinelChecker
}

// NewBrewChecker creates a new Homebrew status checker
func NewBrewChecker() *BrewChecker {
	return &BrewChecker{
		BaseChecker:     &BaseChecker{PowerUpName: "homebrew"},
		SentinelChecker: NewSentinelChecker("homebrew"),
	}
}

// CheckStatus checks if Homebrew packages are installed and up to date
func (bc *BrewChecker) CheckStatus(op *types.Operation, fs filesystem.FullFileSystem) (*types.FileStatus, error) {
	status := bc.InitializeStatus(op.Source, "Brewfile not processed")

	// Extract pack name for metadata
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

	status.Metadata["brewfile"] = op.Source

	// Compute and check sentinel file
	sentinelPath := bc.ComputeSentinelPath(op)
	result := bc.CheckSentinel(fs, sentinelPath, op)

	// Handle sentinel check errors
	if result.Error != nil {
		bc.SetError(status, "check sentinel", result.Error)
		return status, nil
	}

	// Set sentinel metadata
	bc.SetSentinelMetadata(status, result, pack)

	// Handle based on sentinel status
	if !result.Exists {
		status.Status = types.StatusReady
		status.Message = "Brewfile not processed (no sentinel file)"
		return status, nil
	}

	// Sentinel exists, check if we have current checksum
	if result.CurrentChecksum == "" {
		status.Status = types.StatusUnknown
		status.Message = "Cannot determine current Brewfile checksum"
		return status, nil
	}

	// Compare checksums
	if result.StoredChecksum == result.CurrentChecksum {
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
	}

	return status, nil
}
