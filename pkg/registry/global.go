package registry

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// Global registries for different component types
var (
	triggerRegistry  Registry[types.Trigger]
	powerUpRegistry  Registry[types.PowerUp]
	
	// Initialize registries
	_ = func() error {
		triggerRegistry = New[types.Trigger]()
		powerUpRegistry = New[types.PowerUp]()
		return nil
	}()
)

// GetRegistry returns the global registry for the specified type
func GetRegistry[T any]() Registry[T] {
	var zero T
	switch any(zero).(type) {
	case types.Trigger:
		return any(triggerRegistry).(Registry[T])
	case types.PowerUp:
		return any(powerUpRegistry).(Registry[T])
	default:
		// For unknown types, create a new registry
		// This allows for testing and extension
		return New[T]()
	}
}