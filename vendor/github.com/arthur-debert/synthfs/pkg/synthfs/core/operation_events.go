package core

import (
	"time"
)

// Operation event types
const (
	EventOperationStarted   = "operation.started"
	EventOperationCompleted = "operation.completed"
	EventOperationFailed    = "operation.failed"
)

// OperationEventData contains common data for operation events
type OperationEventData struct {
	OperationID   OperationID
	OperationType string
	Path          string
	Details       map[string]interface{}
}

// OperationStartedEvent is emitted when an operation begins execution
type OperationStartedEvent struct {
	*BaseEvent
	Operation OperationEventData
}

// NewOperationStartedEvent creates a new operation started event
func NewOperationStartedEvent(opID OperationID, opType, path string, details map[string]interface{}) *OperationStartedEvent {
	data := OperationEventData{
		OperationID:   opID,
		OperationType: opType,
		Path:          path,
		Details:       details,
	}

	return &OperationStartedEvent{
		BaseEvent: NewBaseEvent(EventOperationStarted, data),
		Operation: data,
	}
}

// OperationCompletedEvent is emitted when an operation completes successfully
type OperationCompletedEvent struct {
	*BaseEvent
	Operation OperationEventData
	Duration  time.Duration
}

// NewOperationCompletedEvent creates a new operation completed event
func NewOperationCompletedEvent(opID OperationID, opType, path string, details map[string]interface{}, duration time.Duration) *OperationCompletedEvent {
	data := OperationEventData{
		OperationID:   opID,
		OperationType: opType,
		Path:          path,
		Details:       details,
	}

	eventData := struct {
		OperationEventData
		Duration time.Duration
	}{
		OperationEventData: data,
		Duration:           duration,
	}

	return &OperationCompletedEvent{
		BaseEvent: NewBaseEvent(EventOperationCompleted, eventData),
		Operation: data,
		Duration:  duration,
	}
}

// OperationFailedEvent is emitted when an operation fails
type OperationFailedEvent struct {
	*BaseEvent
	Operation OperationEventData
	Error     error
	Duration  time.Duration
}

// NewOperationFailedEvent creates a new operation failed event
func NewOperationFailedEvent(opID OperationID, opType, path string, details map[string]interface{}, err error, duration time.Duration) *OperationFailedEvent {
	data := OperationEventData{
		OperationID:   opID,
		OperationType: opType,
		Path:          path,
		Details:       details,
	}

	eventData := struct {
		OperationEventData
		Error    error
		Duration time.Duration
	}{
		OperationEventData: data,
		Error:              err,
		Duration:           duration,
	}

	return &OperationFailedEvent{
		BaseEvent: NewBaseEvent(EventOperationFailed, eventData),
		Operation: data,
		Error:     err,
		Duration:  duration,
	}
}
