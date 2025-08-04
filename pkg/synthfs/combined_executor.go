package synthfs

import (
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
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

// NewCombinedExecutorWithPaths creates a new combined executor with custom paths
func NewCombinedExecutorWithPaths(dryRun bool, p *paths.Paths) *CombinedExecutor {
	return &CombinedExecutor{
		logger:          logging.GetLogger("core.combined_executor"),
		dryRun:          dryRun,
		synthfsExecutor: NewSynthfsExecutorWithPaths(dryRun, p),
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

	// Separate operations into three categories:
	// 1. Filesystem operations (handled by synthfs with automatic dependency resolution)
	// 2. Command operations (must run after filesystem setup)
	// 3. Sentinel file operations (must run after successful command execution)

	var filesystemOps, commandOps, sentinelOps []types.Operation

	for _, op := range operations {
		if op.Status != types.StatusReady {
			continue
		}

		switch op.Type {
		case types.OperationExecute:
			commandOps = append(commandOps, op)
		case types.OperationWriteFile:
			// Check if this is a sentinel file write
			if isSentinelWrite(op) {
				sentinelOps = append(sentinelOps, op)
			} else {
				filesystemOps = append(filesystemOps, op)
			}
		default:
			// All other operations are filesystem operations
			filesystemOps = append(filesystemOps, op)
		}
	}

	// 1. Execute all filesystem operations in one batch
	// The synthfs library will handle dependency resolution (e.g., creating directories before files)
	if len(filesystemOps) > 0 {
		e.logger.Debug().Int("count", len(filesystemOps)).Msg("Executing filesystem operations")
		if err := e.synthfsExecutor.ExecuteOperations(filesystemOps); err != nil {
			return errors.Wrap(err, errors.ErrActionExecute, "failed to execute filesystem operations")
		}
	}

	// 2. Execute commands (must happen after filesystem is set up)
	if len(commandOps) > 0 {
		e.logger.Debug().Int("count", len(commandOps)).Msg("Executing command operations")
		if err := e.commandExecutor.ExecuteOperations(commandOps); err != nil {
			// If command execution fails, don't proceed to sentinel creation
			return errors.Wrap(err, errors.ErrActionExecute, "failed to execute commands")
		}
	}

	// 3. Create sentinel files only after successful command execution
	if len(sentinelOps) > 0 {
		e.logger.Debug().Int("count", len(sentinelOps)).Msg("Creating sentinel files")
		if err := e.synthfsExecutor.ExecuteOperations(sentinelOps); err != nil {
			return errors.Wrap(err, errors.ErrActionExecute, "failed to create sentinel files")
		}
	}

	return nil
}

// isSentinelWrite checks if a write operation is for a sentinel file
func isSentinelWrite(op types.Operation) bool {
	// Check if the description mentions "sentinel" or if the path indicates a sentinel file
	// This is application-specific logic that must be preserved
	return op.Type == types.OperationWriteFile &&
		(strings.Contains(strings.ToLower(op.Description), "sentinel") ||
			strings.Contains(op.Target, "/sentinels/") ||
			strings.Contains(op.Target, "/install/") ||
			strings.Contains(op.Target, "/brewfile/"))
}
