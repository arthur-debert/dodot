package registry

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/types"
)

// Global registries for different component types
var (
	triggerRegistry        Registry[types.Trigger]
	handlerRegistry        Registry[types.Handler]
	triggerFactoryRegistry Registry[types.TriggerFactory]
	handlerFactoryRegistry Registry[types.HandlerFactory]
)

func init() {
	triggerRegistry = New[types.Trigger]()
	handlerRegistry = New[types.Handler]()
	triggerFactoryRegistry = New[types.TriggerFactory]()
	handlerFactoryRegistry = New[types.HandlerFactory]()
}

// GetRegistry returns the global registry for the specified type.
// It uses a type switch to return the correct singleton instance.
func GetRegistry[T any]() Registry[T] {
	var zero T
	switch any(zero).(type) {
	case types.Trigger:
		return any(triggerRegistry).(Registry[T])
	case types.Handler:
		return any(handlerRegistry).(Registry[T])
	case types.TriggerFactory:
		return any(triggerFactoryRegistry).(Registry[T])
	case types.HandlerFactory:
		return any(handlerFactoryRegistry).(Registry[T])
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

// RegisterHandlerFactory registers a factory function for creating handlers.
func RegisterHandlerFactory(name string, factory types.HandlerFactory) error {
	return handlerFactoryRegistry.Register(name, factory)
}

// GetHandlerFactory retrieves a handler factory by name.
func GetHandlerFactory(name string) (types.HandlerFactory, error) {
	factory, err := handlerFactoryRegistry.Get(name)
	if err != nil {
		return nil, fmt.Errorf("handler factory not found: %s", name)
	}
	return factory, nil
}
