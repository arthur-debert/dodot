package core

import (
	"crypto/sha256"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// OperationResult represents the result of executing an operation
type OperationResult struct {
	Operation types.Operation
	Success   bool
	Result    interface{} // For checksum operations, this contains the checksum string
	Error     error
}

// ExecutionContext holds results from operation execution
type ExecutionContext struct {
	ChecksumResults map[string]string // Maps file path to checksum
	Force           bool              // Whether to force operations
	logger          zerolog.Logger
}

// NewExecutionContext creates a new execution context
func NewExecutionContext(force bool) *ExecutionContext {
	return &ExecutionContext{
		ChecksumResults: make(map[string]string),
		Force:           force,
		logger:          logging.GetLogger("core.executor"),
	}
}

// ExecuteChecksumOperations executes only checksum operations and stores results
func (ctx *ExecutionContext) ExecuteChecksumOperations(operations []types.Operation) ([]OperationResult, error) {
	var results []OperationResult

	for _, op := range operations {
		if op.Type != types.OperationChecksum {
			continue // Skip non-checksum operations
		}

		result, err := ctx.executeChecksumOperation(op)
		results = append(results, result)

		if err != nil {
			ctx.logger.Error().
				Err(err).
				Str("source", op.Source).
				Msg("failed to execute checksum operation")
			return results, err
		}

		// Store the checksum result
		if result.Success {
			if checksum, ok := result.Result.(string); ok {
				ctx.ChecksumResults[op.Source] = checksum
				ctx.logger.Debug().
					Str("source", op.Source).
					Str("checksum", checksum).
					Msg("stored checksum result")
			}
		}
	}

	return results, nil
}

// executeChecksumOperation calculates the SHA256 checksum of a file
func (ctx *ExecutionContext) executeChecksumOperation(op types.Operation) (OperationResult, error) {
	if op.Source == "" {
		err := errors.New(errors.ErrActionInvalid, "checksum operation requires source")
		return OperationResult{Operation: op, Success: false, Error: err}, err
	}

	// Expand the source path
	sourcePath := op.Source
	if sourcePath[0] == '~' {
		sourcePath = expandHome(sourcePath)
	}

	// Make path absolute
	if !filepath.IsAbs(sourcePath) {
		if abs, err := filepath.Abs(sourcePath); err == nil {
			sourcePath = abs
		}
	}

	ctx.logger.Debug().
		Str("source", sourcePath).
		Msg("calculating checksum")

	// Check if file exists
	if _, err := os.Stat(sourcePath); os.IsNotExist(err) {
		err := errors.Newf(errors.ErrFileAccess, "file not found: %s", sourcePath)
		return OperationResult{Operation: op, Success: false, Error: err}, err
	}

	// Calculate SHA256 checksum
	checksum, err := calculateFileChecksum(sourcePath)
	if err != nil {
		err := errors.Wrapf(err, errors.ErrFileAccess, "failed to calculate checksum for %s", sourcePath)
		return OperationResult{Operation: op, Success: false, Error: err}, err
	}

	ctx.logger.Info().
		Str("source", sourcePath).
		Str("checksum", checksum).
		Msg("calculated file checksum")

	return OperationResult{
		Operation: op,
		Success:   true,
		Result:    checksum,
		Error:     nil,
	}, nil
}

// GetChecksum returns the stored checksum for a file path
func (ctx *ExecutionContext) GetChecksum(filePath string) (string, bool) {
	// Try the path as-is first
	if checksum, exists := ctx.ChecksumResults[filePath]; exists {
		return checksum, true
	}

	// Try expanding home directory
	if expanded := expandHome(filePath); expanded != filePath {
		if checksum, exists := ctx.ChecksumResults[expanded]; exists {
			return checksum, true
		}
	}

	// Try making absolute
	if abs, err := filepath.Abs(filePath); err == nil {
		if checksum, exists := ctx.ChecksumResults[abs]; exists {
			return checksum, true
		}
	}

	return "", false
}

// calculateFileChecksum calculates the SHA256 checksum of a file
func calculateFileChecksum(filePath string) (string, error) {
	file, err := os.Open(filePath)
	if err != nil {
		return "", err
	}
	defer func() { _ = file.Close() }()

	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return "", err
	}

	return fmt.Sprintf("%x", hash.Sum(nil)), nil
}
