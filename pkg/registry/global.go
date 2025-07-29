package registry

import (
	"fmt"
	
	"github.com/arthur-debert/dodot/pkg/types"
)

// TriggerFactory is a function that creates a Trigger with the given configuration
type TriggerFactory func(config map[string]interface{}) (types.Trigger, error)

// PowerUpFactory is a function that creates a PowerUp with the given configuration
type PowerUpFactory func(config map[string]interface{}) (types.PowerUp, error)

// Global registries for different component types
var (
	triggerRegistry  Registry[types.Trigger]
	powerUpRegistry  Registry[types.PowerUp]
	
	// Factory registries for creating configured instances
	triggerFactoryRegistry Registry[TriggerFactory]
	powerUpFactoryRegistry Registry[PowerUpFactory]
	
	// Initialize registries
	_ = func() error {
		triggerRegistry = New[types.Trigger]()
		powerUpRegistry = New[types.PowerUp]()
		triggerFactoryRegistry = New[TriggerFactory]()
		powerUpFactoryRegistry = New[PowerUpFactory]()
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
	case TriggerFactory:
		return any(triggerFactoryRegistry).(Registry[T])
	case PowerUpFactory:
		return any(powerUpFactoryRegistry).(Registry[T])
	default:
		// For unknown types, create a new registry
		// This allows for testing and extension
		return New[T]()
	}
}

// RegisterTriggerFactory registers a factory function for creating triggers
func RegisterTriggerFactory(name string, factory TriggerFactory) error {
	return triggerFactoryRegistry.Register(name, factory)
}

// GetTriggerFactory retrieves a trigger factory by name
func GetTriggerFactory(name string) (TriggerFactory, error) {
	factory, err := triggerFactoryRegistry.Get(name)
	if err != nil {
		return nil, fmt.Errorf("trigger factory not found: %s", name)
	}
	return factory, nil
}

// RegisterPowerUpFactory registers a factory function for creating power-ups
func RegisterPowerUpFactory(name string, factory PowerUpFactory) error {
	return powerUpFactoryRegistry.Register(name, factory)
}

// GetPowerUpFactory retrieves a power-up factory by name
func GetPowerUpFactory(name string) (PowerUpFactory, error) {
	factory, err := powerUpFactoryRegistry.Get(name)
	if err != nil {
		return nil, fmt.Errorf("power-up factory not found: %s", name)
	}
	return factory, nil
}