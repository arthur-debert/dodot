package types

// LinkingHandler generates actions that are idempotent and fast.
// These handlers create configuration links that can be safely run multiple times.
type LinkingHandler interface {
	// Name returns the unique name of this handler
	Name() string

	// Description returns a human-readable description of what this handler does
	Description() string

	// RunMode returns whether this handler runs once or many times
	RunMode() RunMode

	// ValidateOptions checks if the provided options are valid for this handler
	ValidateOptions(options map[string]interface{}) error

	// GetTemplateContent returns the template content for this handler
	// Returns empty string if the handler doesn't provide a template
	GetTemplateContent() string

	// ProcessLinking generates linking actions from the matched files
	ProcessLinking(matches []TriggerMatch) ([]LinkingAction, error)
}

// ProvisioningHandler generates actions that have side effects.
// These handlers typically run once to install software or perform system changes.
type ProvisioningHandler interface {
	// Name returns the unique name of this handler
	Name() string

	// Description returns a human-readable description of what this handler does
	Description() string

	// RunMode returns whether this handler runs once or many times
	RunMode() RunMode

	// ValidateOptions checks if the provided options are valid for this handler
	ValidateOptions(options map[string]interface{}) error

	// GetTemplateContent returns the template content for this handler
	// Returns empty string if the handler doesn't provide a template
	GetTemplateContent() string

	// ProcessProvisioning generates provisioning actions from the matched files
	ProcessProvisioning(matches []TriggerMatch) ([]ProvisioningAction, error)
}

// DualModeHandler is a handler that can operate in both linking and provisioning modes
type DualModeHandler interface {
	LinkingHandler
	ProvisioningHandler
}
