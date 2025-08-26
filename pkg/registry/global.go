package registry

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/types"
)

// HandlerFactory creates a new handler instance with the given options
type HandlerFactory func(options map[string]interface{}) (interface{}, error)

// Global registries for different component types
var (
	triggerRegistry        Registry[types.Trigger]
	triggerFactoryRegistry Registry[types.TriggerFactory]
	handlerFactoryRegistry Registry[HandlerFactory]
)

func init() {
	triggerRegistry = New[types.Trigger]()
	triggerFactoryRegistry = New[types.TriggerFactory]()
	handlerFactoryRegistry = New[HandlerFactory]()
}

// GetRegistry returns the global registry for the specified type.
// It uses a type switch to return the correct singleton instance.
func GetRegistry[T any]() Registry[T] {
	var zero T
	switch any(zero).(type) {
	case types.Trigger:
		return any(triggerRegistry).(Registry[T])
	case types.TriggerFactory:
		return any(triggerFactoryRegistry).(Registry[T])
	case HandlerFactory:
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
func RegisterHandlerFactory(name string, factory HandlerFactory) error {
	return handlerFactoryRegistry.Register(name, factory)
}

// GetHandlerFactory retrieves a handler factory by name.
func GetHandlerFactory(name string) (HandlerFactory, error) {
	factory, err := handlerFactoryRegistry.Get(name)
	if err != nil {
		return nil, fmt.Errorf("handler factory not found: %s", name)
	}
	return factory, nil
}

// GetLinkingHandler creates a linking handler instance by name with the given options.
func GetLinkingHandler(name string, options map[string]interface{}) (handlers.LinkingHandler, error) {
	factory, err := GetHandlerFactory(name)
	if err != nil {
		return nil, err
	}

	handler, err := factory(options)
	if err != nil {
		return nil, fmt.Errorf("failed to create handler %s: %w", name, err)
	}

	linkingHandler, ok := handler.(handlers.LinkingHandler)
	if !ok {
		return nil, fmt.Errorf("handler %s does not implement LinkingHandler interface", name)
	}

	return linkingHandler, nil
}

// GetProvisioningHandler creates a provisioning handler instance by name with the given options.
func GetProvisioningHandler(name string, options map[string]interface{}) (handlers.ProvisioningHandler, error) {
	factory, err := GetHandlerFactory(name)
	if err != nil {
		return nil, err
	}

	handler, err := factory(options)
	if err != nil {
		return nil, fmt.Errorf("failed to create handler %s: %w", name, err)
	}

	provisioningHandler, ok := handler.(handlers.ProvisioningHandler)
	if !ok {
		return nil, fmt.Errorf("handler %s does not implement ProvisioningHandler interface", name)
	}

	return provisioningHandler, nil
}
