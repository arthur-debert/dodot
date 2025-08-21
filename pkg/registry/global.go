package registry

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/types"
)

// Global registries for different component types
var (
	triggerRegistry        Registry[types.Trigger]
	powerUpRegistry        Registry[types.Handler]
	triggerFactoryRegistry Registry[types.TriggerFactory]
	powerUpFactoryRegistry Registry[types.HandlerFactory]
)

func init() {
	triggerRegistry = New[types.Trigger]()
	powerUpRegistry = New[types.Handler]()
	triggerFactoryRegistry = New[types.TriggerFactory]()
	powerUpFactoryRegistry = New[types.HandlerFactory]()
}

// GetRegistry returns the global registry for the specified type.
// It uses a type switch to return the correct singleton instance.
func GetRegistry[T any]() Registry[T] {
	var zero T
	switch any(zero).(type) {
	case types.Trigger:
		return any(triggerRegistry).(Registry[T])
	case types.Handler:
		return any(powerUpRegistry).(Registry[T])
	case types.TriggerFactory:
		return any(triggerFactoryRegistry).(Registry[T])
	case types.HandlerFactory:
		return any(powerUpFactoryRegistry).(Registry[T])
	default:
		// This should ideally not be reached in production code,
		// but can be useful for tests with novel types.
		// For core types, it's better to have them explicitly in the switch.
		return New[T]()
	}
}

// RegisterTriggerFactory registers a factory function for creating triggers.
func RegisterTriggerFactory(name string, factory types.TriggerFactory) error {
	return triggerFactoryRegistry.Register(name, factory)
}

// GetTriggerFactory retrieves a trigger factory by name.
func GetTriggerFactory(name string) (types.TriggerFactory, error) {
	factory, err := triggerFactoryRegistry.Get(name)
	if err != nil {
		return nil, fmt.Errorf("trigger factory not found: %s", name)
	}
	return factory, nil
}

// RegisterHandlerFactory registers a factory function for creating power-ups.
func RegisterHandlerFactory(name string, factory types.HandlerFactory) error {
	return powerUpFactoryRegistry.Register(name, factory)
}

// GetHandlerFactory retrieves a power-up factory by name.
func GetHandlerFactory(name string) (types.HandlerFactory, error) {
	factory, err := powerUpFactoryRegistry.Get(name)
	if err != nil {
		return nil, fmt.Errorf("power-up factory not found: %s", name)
	}
	return factory, nil
}
