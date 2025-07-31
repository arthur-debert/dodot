package core

import (
	"context"
	"fmt"
	"sync"
	"time"
)

// Event represents a system event
type Event interface {
	// Type returns the event type identifier
	Type() string
	// Timestamp returns when the event occurred
	Timestamp() time.Time
	// Data returns the event payload
	Data() interface{}
}

// EventHandler handles events
type EventHandler interface {
	// Handle processes an event
	Handle(ctx context.Context, event Event) error
}

// EventHandlerFunc is a function adapter for EventHandler
type EventHandlerFunc func(ctx context.Context, event Event) error

// Handle implements EventHandler
func (f EventHandlerFunc) Handle(ctx context.Context, event Event) error {
	return f(ctx, event)
}

// SubscriptionID identifies a subscription
type SubscriptionID string

// EventBus manages event publishing and subscription
type EventBus interface {
	// Subscribe registers a handler for events of the given type, returns subscription ID
	Subscribe(eventType string, handler EventHandler) SubscriptionID
	// Unsubscribe removes a handler using its subscription ID
	Unsubscribe(subscriptionID SubscriptionID)
	// Publish sends an event to all registered handlers
	Publish(ctx context.Context, event Event) error
	// PublishAsync sends an event to all registered handlers asynchronously
	PublishAsync(ctx context.Context, event Event)
}

// BaseEvent provides a basic implementation of Event
type BaseEvent struct {
	EventType string
	Time      time.Time
	Payload   interface{}
}

// Type returns the event type
func (e *BaseEvent) Type() string {
	return e.EventType
}

// Timestamp returns when the event occurred
func (e *BaseEvent) Timestamp() time.Time {
	return e.Time
}

// Data returns the event payload
func (e *BaseEvent) Data() interface{} {
	return e.Payload
}

// NewBaseEvent creates a new base event
func NewBaseEvent(eventType string, data interface{}) *BaseEvent {
	return &BaseEvent{
		EventType: eventType,
		Time:      time.Now(),
		Payload:   data,
	}
}

// subscription holds a handler and its metadata
type subscription struct {
	id      SubscriptionID
	handler EventHandler
}

// MemoryEventBus is an in-memory implementation of EventBus
type MemoryEventBus struct {
	mu            sync.RWMutex
	handlers      map[string][]subscription
	subscriptions map[SubscriptionID]string // maps subscription ID to event type
	nextID        int
	logger        Logger
}

// NewMemoryEventBus creates a new in-memory event bus
func NewMemoryEventBus(logger Logger) *MemoryEventBus {
	return &MemoryEventBus{
		handlers:      make(map[string][]subscription),
		subscriptions: make(map[SubscriptionID]string),
		nextID:        1,
		logger:        logger,
	}
}

// Subscribe registers a handler for events of the given type
func (bus *MemoryEventBus) Subscribe(eventType string, handler EventHandler) SubscriptionID {
	bus.mu.Lock()
	defer bus.mu.Unlock()

	// Generate unique subscription ID
	subID := SubscriptionID(fmt.Sprintf("sub_%d", bus.nextID))
	bus.nextID++

	// Create subscription
	sub := subscription{
		id:      subID,
		handler: handler,
	}

	bus.handlers[eventType] = append(bus.handlers[eventType], sub)
	bus.subscriptions[subID] = eventType

	bus.logger.Debug().
		Str("event_type", eventType).
		Str("subscription_id", string(subID)).
		Int("total_handlers", len(bus.handlers[eventType])).
		Msg("subscribed to event")

	return subID
}

// Unsubscribe removes a handler using its subscription ID
func (bus *MemoryEventBus) Unsubscribe(subscriptionID SubscriptionID) {
	bus.mu.Lock()
	defer bus.mu.Unlock()

	eventType, exists := bus.subscriptions[subscriptionID]
	if !exists {
		bus.logger.Debug().
			Str("subscription_id", string(subscriptionID)).
			Msg("subscription not found for unsubscribe")
		return
	}

	// Remove from subscriptions map
	delete(bus.subscriptions, subscriptionID)

	// Remove from handlers
	handlers := bus.handlers[eventType]
	for i, sub := range handlers {
		if sub.id == subscriptionID {
			// Remove subscription by swapping with last element and truncating
			handlers[i] = handlers[len(handlers)-1]
			bus.handlers[eventType] = handlers[:len(handlers)-1]

			bus.logger.Debug().
				Str("event_type", eventType).
				Str("subscription_id", string(subscriptionID)).
				Int("remaining_handlers", len(bus.handlers[eventType])).
				Msg("unsubscribed from event")
			return
		}
	}
}

// Publish sends an event to all registered handlers synchronously
func (bus *MemoryEventBus) Publish(ctx context.Context, event Event) error {
	bus.mu.RLock()
	subscriptions := append([]subscription{}, bus.handlers[event.Type()]...)
	bus.mu.RUnlock()

	if len(subscriptions) == 0 {
		bus.logger.Trace().
			Str("event_type", event.Type()).
			Msg("no handlers for event")
		return nil
	}

	bus.logger.Debug().
		Str("event_type", event.Type()).
		Int("handler_count", len(subscriptions)).
		Msg("publishing event")

	// Execute handlers synchronously
	for _, sub := range subscriptions {
		if err := sub.handler.Handle(ctx, event); err != nil {
			bus.logger.Warn().
				Str("event_type", event.Type()).
				Str("subscription_id", string(sub.id)).
				Err(err).
				Msg("event handler failed")
			// Continue with other handlers even if one fails
		}
	}

	return nil
}

// PublishAsync sends an event to all registered handlers asynchronously
func (bus *MemoryEventBus) PublishAsync(ctx context.Context, event Event) {
	go func() {
		if err := bus.Publish(ctx, event); err != nil {
			bus.logger.Error().
				Str("event_type", event.Type()).
				Err(err).
				Msg("async event publishing failed")
		}
	}()
}
