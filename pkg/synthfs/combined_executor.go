package synthfs

import (
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// CombinedExecutor handles both file system and command operations in the correct order
type CombinedExecutor struct {
	logger          zerolog.Logger
	dryRun          bool
	synthfsExecutor *SynthfsExecutor
	commandExecutor *CommandExecutor
}

// NewCombinedExecutor creates a new combined executor
func NewCombinedExecutor(dryRun bool) *CombinedExecutor {
	return &CombinedExecutor{
		logger:          logging.GetLogger("core.combined_executor"),
		dryRun:          dryRun,
		synthfsExecutor: NewSynthfsExecutor(dryRun),
		commandExecutor: NewCommandExecutor(dryRun),
	}
}

// EnableHomeSymlinks allows the executor to create symlinks in the user's home directory
func (e *CombinedExecutor) EnableHomeSymlinks(backup bool) *CombinedExecutor {
	e.synthfsExecutor.EnableHomeSymlinks(backup)
	return e
}

// ExecuteOperations executes operations in the correct order, handling dependencies
func (e *CombinedExecutor) ExecuteOperations(operations []types.Operation) error {
	if len(operations) == 0 {
		return nil
	}

	// Group operations by their dependencies
	// For install/brew actions, we need to ensure:
	// 1. Directory creation happens first
	// 2. Script execution happens next
	// 3. Sentinel file creation happens only after successful execution

	// First, separate operations by type
	var dirOps, executeOps, fileOps, otherOps []types.Operation

	for _, op := range operations {
		if op.Status != types.StatusReady {
			continue
		}

		switch op.Type {
		case types.OperationCreateDir:
			dirOps = append(dirOps, op)
		case types.OperationExecute:
			executeOps = append(executeOps, op)
		case types.OperationWriteFile:
			// Check if this is a sentinel file write
			if isSentinelWrite(op) {
				// Sentinel writes go after execution
				fileOps = append(fileOps, op)
			} else {
				// Other file writes can happen with other ops
				otherOps = append(otherOps, op)
			}
		default:
			otherOps = append(otherOps, op)
		}
	}

	// Execute in order:
	// 1. Create directories first
	if len(dirOps) > 0 {
		e.logger.Debug().Int("count", len(dirOps)).Msg("Executing directory operations")
		if err := e.synthfsExecutor.ExecuteOperations(dirOps); err != nil {
			return errors.Wrap(err, errors.ErrActionExecute, "failed to create directories")
		}
	}

	// 2. Execute other file system operations (symlinks, non-sentinel writes, etc)
	if len(otherOps) > 0 {
		e.logger.Debug().Int("count", len(otherOps)).Msg("Executing other file operations")
		if err := e.synthfsExecutor.ExecuteOperations(otherOps); err != nil {
			return errors.Wrap(err, errors.ErrActionExecute, "failed to execute file operations")
		}
	}

	// 3. Execute commands
	if len(executeOps) > 0 {
		e.logger.Debug().Int("count", len(executeOps)).Msg("Executing command operations")
		if err := e.commandExecutor.ExecuteOperations(executeOps); err != nil {
			// If command execution fails, don't proceed to sentinel creation
			return errors.Wrap(err, errors.ErrActionExecute, "failed to execute commands")
		}
	}

	// 4. Create sentinel files only after successful execution
	if len(fileOps) > 0 {
		e.logger.Debug().Int("count", len(fileOps)).Msg("Creating sentinel files")
		if err := e.synthfsExecutor.ExecuteOperations(fileOps); err != nil {
			return errors.Wrap(err, errors.ErrActionExecute, "failed to create sentinel files")
		}
	}

	return nil
}

// isSentinelWrite checks if a write operation is for a sentinel file
func isSentinelWrite(op types.Operation) bool {
	// Check if the description mentions "sentinel"
	// This is a simple heuristic - we could also check the path
	return op.Type == types.OperationWriteFile &&
		(contains(op.Description, "sentinel") ||
			contains(op.Target, "/sentinels/") ||
			contains(op.Target, "/install/") ||
			contains(op.Target, "/brewfile/"))
}

// contains is a simple string contains helper
func contains(s, substr string) bool {
	return len(s) >= len(substr) && containsSubstring(s, substr)
}

func containsSubstring(s, substr string) bool {
	if len(substr) == 0 {
		return true
	}
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}
