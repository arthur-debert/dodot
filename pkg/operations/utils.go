package operations

import (
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AreOperationsCompatible checks if a set of operations can coexist without conflicts
func AreOperationsCompatible(ops []*types.Operation) bool {
	if len(ops) <= 1 {
		return true
	}
	allDirCreates := true
	for _, op := range ops {
		if op.Type != types.OperationCreateDir {
			allDirCreates = false
			break
		}
	}
	return allDirCreates
}

// ExpandHome expands the home directory in a path
func ExpandHome(path string) string {
	return paths.ExpandHome(path)
}

// Uint32Ptr returns a pointer to a uint32 value
func Uint32Ptr(v uint32) *uint32 {
	return &v
}

// DeduplicateOperations removes duplicate operations from a slice
// Operations are considered duplicates if they have the same type and target
func DeduplicateOperations(ops []types.Operation) []types.Operation {
	if len(ops) <= 1 {
		return ops
	}

	logger := logging.GetLogger("operations.utils")
	seen := make(map[string]bool)
	result := make([]types.Operation, 0, len(ops))

	for _, op := range ops {
		// Create a key based on operation type and target
		// This ensures operations with same type and target are considered duplicates
		key := string(op.Type) + ":" + op.Target

		if !seen[key] {
			seen[key] = true
			result = append(result, op)
		} else {
			// Log when we skip a duplicate operation
			logger.Warn().
				Str("type", string(op.Type)).
				Str("target", op.Target).
				Str("description", op.Description).
				Msg("Skipping duplicate operation")
		}
	}

	return result
}
