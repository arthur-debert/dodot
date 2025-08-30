package operations

import "os"

// UseSimplifiedHandlers checks if the simplified handler system should be used.
// This is controlled by the DODOT_USE_OPERATIONS environment variable.
// Set DODOT_USE_OPERATIONS=true to enable the new system for testing.
func UseSimplifiedHandlers() bool {
	return os.Getenv("DODOT_USE_OPERATIONS") == "true"
}

// IsHandlerSimplified checks if a specific handler has been migrated.
// Phase 2: Migrating all handlers one by one.
func IsHandlerSimplified(handlerName string) bool {
	if !UseSimplifiedHandlers() {
		return false
	}

	// Phase 1: Path handler (completed)
	// Phase 2: Migrating remaining handlers
	switch handlerName {
	case HandlerPath, HandlerSymlink:
		return true
	default:
		return false
	}
}
