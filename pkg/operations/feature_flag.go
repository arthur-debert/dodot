package operations

// UseSimplifiedHandlers checks if the simplified handler system should be used.
// Phase 3: This now always returns true - operations are the default.
func UseSimplifiedHandlers() bool {
	return true
}

// IsHandlerSimplified checks if a specific handler has been migrated.
// Phase 3: All handlers are now simplified.
func IsHandlerSimplified(handlerName string) bool {
	return true
}
