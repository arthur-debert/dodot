package operations

import "os"

// UseSimplifiedHandlers checks if the simplified handler system should be used.
// This is controlled by the DODOT_USE_OPERATIONS environment variable.
// Set DODOT_USE_OPERATIONS=true to enable the new system for testing.
func UseSimplifiedHandlers() bool {
	return os.Getenv("DODOT_USE_OPERATIONS") == "true"
}

// IsHandlerSimplified checks if a specific handler has been migrated.
// During phase 1, only the path handler is migrated as proof of concept.
func IsHandlerSimplified(handlerName string) bool {
	if !UseSimplifiedHandlers() {
		return false
	}

	// Phase 1: Only path handler is migrated
	switch handlerName {
	case HandlerPath:
		return true
	default:
		return false
	}
}
