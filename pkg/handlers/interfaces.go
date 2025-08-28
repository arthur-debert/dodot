package handlers

import "github.com/arthur-debert/dodot/pkg/types"

// Handler is the base interface that all handlers must implement.
// It provides common methods for identifying and describing handlers.
type Handler interface {
	// Name returns the unique name of this handler
	Name() string

	// Description returns a human-readable description of what this handler does
	Description() string

	// Type returns the fundamental nature of this handler's operations
	Type() types.HandlerType

	// RunMode returns whether this handler runs once or many times
	// Deprecated: Use Type() instead. This method will be removed in a future version.
	RunMode() types.RunMode
}

// LinkingHandler generates actions that are idempotent and fast.
// These handlers create configuration links that can be safely run multiple times.
type LinkingHandler interface {
	Handler

	// ValidateOptions checks if the provided options are valid for this handler
	ValidateOptions(options map[string]interface{}) error

	// GetTemplateContent returns the template content for this handler
	// Returns empty string if the handler doesn't provide a template
	GetTemplateContent() string

	// ProcessLinking generates linking actions from the matched files
	ProcessLinking(matches []types.RuleMatch) ([]types.LinkingAction, error)
}

// ProvisioningHandler generates actions that have side effects.
// These handlers typically run once to install software or perform system changes.
type ProvisioningHandler interface {
	Handler

	// ValidateOptions checks if the provided options are valid for this handler
	ValidateOptions(options map[string]interface{}) error

	// GetTemplateContent returns the template content for this handler
	// Returns empty string if the handler doesn't provide a template
	GetTemplateContent() string

	// ProcessProvisioning generates provisioning actions from the matched files
	ProcessProvisioning(matches []types.RuleMatch) ([]types.ProvisioningAction, error)
}

// DualModeHandler is a handler that can operate in both linking and provisioning modes
type DualModeHandler interface {
	LinkingHandler
	ProvisioningHandler
}

// LinkingHandlerWithConfirmations is an optional interface that linking handlers can implement
// to support confirmation requests. Handlers implementing this interface can generate
// both actions and confirmation requests during processing.
type LinkingHandlerWithConfirmations interface {
	LinkingHandler

	// ProcessLinkingWithConfirmations generates linking actions and confirmation requests
	// from the matched files. This replaces ProcessLinking for handlers that need confirmations.
	ProcessLinkingWithConfirmations(matches []types.RuleMatch) (types.ProcessingResult, error)
}

// ProvisioningHandlerWithConfirmations is an optional interface that provisioning handlers can implement
// to support confirmation requests. Handlers implementing this interface can generate
// both actions and confirmation requests during processing.
type ProvisioningHandlerWithConfirmations interface {
	ProvisioningHandler

	// ProcessProvisioningWithConfirmations generates provisioning actions and confirmation requests
	// from the matched files. This replaces ProcessProvisioning for handlers that need confirmations.
	ProcessProvisioningWithConfirmations(matches []types.RuleMatch) (types.ProcessingResult, error)
}

// Clearable represents a handler that can clean up its deployments
type Clearable interface {
	// Clear performs handler-specific cleanup.
	// Handlers should read their state, perform cleanup, and return what was cleared.
	// The state directory will be removed AFTER this method completes successfully.
	// If dryRun is true, handlers should report what would be cleared without actually doing it.
	Clear(ctx types.ClearContext) ([]types.ClearedItem, error)
}

// ClearableWithConfirmations is an optional interface that clearable handlers can implement
// to support confirmation requests for clear operations.
type ClearableWithConfirmations interface {
	Clearable

	// ClearWithConfirmations performs handler-specific cleanup with pre-collected confirmations.
	// The confirmations parameter contains user responses to confirmation requests.
	// If no confirmations were needed, confirmations will be nil.
	ClearWithConfirmations(ctx types.ClearContext, confirmations *types.ConfirmationContext) ([]types.ClearedItem, error)

	// GetClearConfirmations returns confirmation requests needed for clearing this handler's state.
	// This is called before ClearWithConfirmations to collect user approval upfront.
	// If no confirmations are needed, return empty ProcessingResult with no confirmations.
	GetClearConfirmations(ctx types.ClearContext) ([]types.ConfirmationRequest, error)
}
