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

// ClearedItem represents something that was removed during a clear operation
type ClearedItem struct {
	Type        string // "symlink", "brew_package", "script_output", etc.
	Path        string // What was removed/affected
	Description string // Human-readable description
}

// Paths is the interface for path resolution
// This is a subset of paths.Paths to avoid circular imports
type Paths interface {
	PackHandlerDir(packName, handlerName string) string
	MapPackFileToSystem(pack *Pack, relPath string) string
}

// ClearContext provides all the resources needed for a handler to clean up
type ClearContext struct {
	Pack      Pack      // The pack being cleared
	DataStore DataStore // For accessing state information
	FS        FS        // For file operations
	Paths     Paths     // For path resolution
	DryRun    bool      // Whether this is a dry run
}

// Clearable represents a handler that can clean up its deployments
type Clearable interface {
	// Clear performs handler-specific cleanup.
	// Handlers should read their state, perform cleanup, and return what was cleared.
	// The state directory will be removed AFTER this method completes successfully.
	// If dryRun is true, handlers should report what would be cleared without actually doing it.
	Clear(ctx ClearContext) ([]ClearedItem, error)
}
