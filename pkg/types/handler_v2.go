package types

// LinkingHandlerV2 generates actions that are idempotent and fast.
// These handlers create configuration links that can be safely run multiple times.
type LinkingHandlerV2 interface {
	Handler

	// ProcessLinking generates linking actions from the matched files
	ProcessLinking(matches []TriggerMatch) ([]LinkingAction, error)
}

// ProvisioningHandlerV2 generates actions that have side effects.
// These handlers typically run once to install software or perform system changes.
type ProvisioningHandlerV2 interface {
	Handler

	// ProcessProvisioning generates provisioning actions from the matched files
	ProcessProvisioning(matches []TriggerMatch) ([]ProvisioningAction, error)
}

// DualModeHandlerV2 is a handler that can operate in both linking and provisioning modes
type DualModeHandlerV2 interface {
	Handler
	LinkingHandlerV2
	ProvisioningHandlerV2
}
