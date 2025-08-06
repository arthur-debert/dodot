package status

import (
	"fmt"
	"io"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// SentinelChecker provides common functionality for checking sentinel files
type SentinelChecker struct {
	PowerUpName string
}

// NewSentinelChecker creates a new sentinel checker
func NewSentinelChecker(powerUpName string) *SentinelChecker {
	return &SentinelChecker{
		PowerUpName: powerUpName,
	}
}

// ComputeSentinelPath computes the sentinel file path based on the operation
func (sc *SentinelChecker) ComputeSentinelPath(op *types.Operation) string {
	// Extract pack name from metadata
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

	// For sentinel write operations, use the target directly
	if op.Type == types.OperationWriteFile && strings.Contains(op.Target, fmt.Sprintf("/%s/", sc.PowerUpName)) {
		return op.Target
	}

	// Otherwise compute the sentinel path
	return filepath.Join(filepath.Dir(op.Target), pack)
}

// CheckSentinelResult contains the result of checking a sentinel file
type CheckSentinelResult struct {
	Exists          bool
	StoredChecksum  string
	CurrentChecksum string
	Error           error
}

// CheckSentinel checks if a sentinel file exists and reads its checksum
func (sc *SentinelChecker) CheckSentinel(fs filesystem.FullFileSystem, sentinelPath string, op *types.Operation) *CheckSentinelResult {
	result := &CheckSentinelResult{}

	// Get current checksum from operation first (always needed)
	result.CurrentChecksum = sc.GetCurrentChecksum(op)

	// Check if sentinel file exists
	_, err := fs.Stat(sentinelPath)
	if err != nil {
		if isNotExist(err) {
			result.Exists = false
			return result
		}
		result.Error = fmt.Errorf("check sentinel file: %w", err)
		return result
	}

	result.Exists = true

	// Read sentinel file to get stored checksum
	reader, err := fs.Open(sentinelPath)
	if err != nil {
		result.Error = fmt.Errorf("open sentinel file: %w", err)
		return result
	}
	defer func() {
		_ = reader.Close()
	}()

	storedChecksumBytes, err := io.ReadAll(reader)
	if err != nil {
		result.Error = fmt.Errorf("read sentinel checksum: %w", err)
		return result
	}

	result.StoredChecksum = strings.TrimSpace(string(storedChecksumBytes))

	return result
}

// GetCurrentChecksum extracts the current checksum from the operation
func (sc *SentinelChecker) GetCurrentChecksum(op *types.Operation) string {
	// For sentinel write operations, content is the checksum
	if op.Content != "" {
		return op.Content
	}

	// Otherwise check metadata
	if op.Metadata != nil {
		if cs, ok := op.Metadata["checksum"].(string); ok {
			return cs
		}
	}

	return ""
}

// SetSentinelMetadata sets sentinel-related metadata on the status
func (sc *SentinelChecker) SetSentinelMetadata(status *types.FileStatus, result *CheckSentinelResult, pack string) {
	status.Metadata["pack"] = pack
	status.Metadata["sentinel_exists"] = result.Exists

	if result.Exists {
		status.Metadata["stored_checksum"] = result.StoredChecksum
	}

	if result.CurrentChecksum != "" {
		status.Metadata["current_checksum"] = result.CurrentChecksum
	}

	if result.Exists && result.CurrentChecksum != "" {
		status.Metadata["checksum_match"] = result.StoredChecksum == result.CurrentChecksum
	}
}
