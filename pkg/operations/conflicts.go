package operations

import (
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ForceChecker defines a minimal interface for checking force mode
type ForceChecker interface {
	IsForce() bool
}

// ResolveConflicts checks for and marks conflicting operations
// It handles both filesystem conflicts and internal operation conflicts
func ResolveConflicts(operations *[]types.Operation, ctx ForceChecker) {
	logger := logging.GetLogger("operations.conflicts")
	ops := *operations
	var force bool
	if ctx != nil {
		force = ctx.IsForce()
	}
	processedTargets := make(map[string]bool)

	for i := range ops {
		op := &ops[i]
		if op.Status != types.StatusReady {
			continue
		}

		target := filepath.Clean(op.Target)
		if target == "" {
			continue
		}

		if processedTargets[target] {
			if !force {
				op.Status = types.StatusConflict
			}
			continue
		}

		// Check for filesystem conflicts
		if op.Type == types.OperationCreateSymlink {
			if _, err := os.Lstat(op.Target); err == nil {
				if !force {
					op.Status = types.StatusConflict
					logger.Debug().
						Str("target", op.Target).
						Msg("Marking symlink operation as conflicted due to existing file")
				}
			} else if !os.IsNotExist(err) {
				op.Status = types.StatusError
				logger.Error().
					Err(err).
					Str("target", op.Target).
					Msg("Error checking symlink target")
			}
		}

		if op.Status == types.StatusReady {
			processedTargets[target] = true
		}
	}

	*operations = ops
}

// ResolveOperationConflicts is an alternative conflict resolution implementation
// It checks for both internal conflicts between operations and filesystem conflicts
func ResolveOperationConflicts(operations *[]types.Operation, ctx ForceChecker) {
	logger := logging.GetLogger("operations.conflicts")
	ops := *operations
	var force bool
	if ctx != nil {
		force = ctx.IsForce()
	}

	for i := range ops {
		op := &ops[i]
		if op.Status != types.StatusReady {
			continue
		}

		// Check for internal conflicts (multiple ops targeting the same path)
		for j := i + 1; j < len(ops); j++ {
			otherOp := &ops[j]
			if op.Target == otherOp.Target && !AreOperationsCompatible([]*types.Operation{op, otherOp}) {
				if !force {
					logger.Error().
						Str("target", op.Target).
						Msg("Incompatible operations targeting the same path")
					op.Status = types.StatusConflict
					otherOp.Status = types.StatusConflict
				}
			}
		}

		// Check for filesystem conflicts (e.g., pre-existing files)
		if op.Type == types.OperationCreateSymlink {
			if _, err := os.Lstat(op.Target); err == nil {
				if !force {
					logger.Warn().
						Str("target", op.Target).
						Msg("Target file exists and --force is not used, marking as conflict")
					op.Status = types.StatusConflict
				}
			} else if !os.IsNotExist(err) {
				logger.Error().Err(err).Str("target", op.Target).Msg("Failed to check target file status")
				op.Status = types.StatusError
			}
		}
	}

	*operations = ops
}
