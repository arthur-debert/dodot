package types

// LinkingHandlerWithConfirmations is an optional interface that linking handlers can implement
// to support confirmation requests. Handlers implementing this interface can generate
// both actions and confirmation requests during processing.
type LinkingHandlerWithConfirmations interface {
	LinkingHandler

	// ProcessLinkingWithConfirmations generates linking actions and confirmation requests
	// from the matched files. This replaces ProcessLinking for handlers that need confirmations.
	ProcessLinkingWithConfirmations(matches []TriggerMatch) (ProcessingResult, error)
}

// ProvisioningHandlerWithConfirmations is an optional interface that provisioning handlers can implement
// to support confirmation requests. Handlers implementing this interface can generate
// both actions and confirmation requests during processing.
type ProvisioningHandlerWithConfirmations interface {
	ProvisioningHandler

	// ProcessProvisioningWithConfirmations generates provisioning actions and confirmation requests
	// from the matched files. This replaces ProcessProvisioning for handlers that need confirmations.
	ProcessProvisioningWithConfirmations(matches []TriggerMatch) (ProcessingResult, error)
}

// ClearableWithConfirmations is an optional interface that clearable handlers can implement
// to support confirmation requests for clear operations.
type ClearableWithConfirmations interface {
	Clearable

	// ClearWithConfirmations performs handler-specific cleanup with pre-collected confirmations.
	// The confirmations parameter contains user responses to confirmation requests.
	// If no confirmations were needed, confirmations will be nil.
	ClearWithConfirmations(ctx ClearContext, confirmations *ConfirmationContext) ([]ClearedItem, error)

	// GetClearConfirmations returns confirmation requests needed for clearing this handler's state.
	// This is called before ClearWithConfirmations to collect user approval upfront.
	// If no confirmations are needed, return empty ProcessingResult with no confirmations.
	GetClearConfirmations(ctx ClearContext) ([]ConfirmationRequest, error)
}

// Helper functions for handlers to create confirmation requests

// NewConfirmationRequest creates a new confirmation request with all fields set
func NewConfirmationRequest(id, pack, handler, operation, title, description string, items []string, defaultResponse bool) ConfirmationRequest {
	return ConfirmationRequest{
		ID:          id,
		Pack:        pack,
		Handler:     handler,
		Operation:   operation,
		Title:       title,
		Description: description,
		Items:       items,
		Default:     defaultResponse,
	}
}

// NewClearConfirmationRequest creates a confirmation request specifically for clear operations
func NewClearConfirmationRequest(id, pack, handler, title, description string, items []string, defaultResponse bool) ConfirmationRequest {
	return NewConfirmationRequest(id, pack, handler, "clear", title, description, items, defaultResponse)
}

// NewProvisionConfirmationRequest creates a confirmation request specifically for provision operations
func NewProvisionConfirmationRequest(id, pack, handler, title, description string, items []string, defaultResponse bool) ConfirmationRequest {
	return NewConfirmationRequest(id, pack, handler, "provision", title, description, items, defaultResponse)
}

// IsLinkingHandlerWithConfirmations checks if a handler supports linking confirmations
func IsLinkingHandlerWithConfirmations(handler interface{}) bool {
	_, ok := handler.(LinkingHandlerWithConfirmations)
	return ok
}

// IsProvisioningHandlerWithConfirmations checks if a handler supports provisioning confirmations
func IsProvisioningHandlerWithConfirmations(handler interface{}) bool {
	_, ok := handler.(ProvisioningHandlerWithConfirmations)
	return ok
}

// IsClearableWithConfirmations checks if a handler supports clear confirmations
func IsClearableWithConfirmations(handler interface{}) bool {
	_, ok := handler.(ClearableWithConfirmations)
	return ok
}
