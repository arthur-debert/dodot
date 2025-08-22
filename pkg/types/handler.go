package types

// RunMode indicates how often a handler should be executed
type RunMode string

const (
	// RunModeLinking indicates the handler creates/updates links and can be run multiple times safely
	RunModeLinking RunMode = "linking"

	// RunModeProvisioning indicates the handler provisions resources and should only run once per pack
	RunModeProvisioning RunMode = "provisioning"
)

const (
	// OverridePriority is a high priority value for config overrides
	OverridePriority = 100
)

// Handler is an interface for action generators that process matched files.
// Handlers receive groups of files that their associated triggers matched,
// and generate high-level actions describing what should be done.
type Handler interface {
	// Name returns the unique name of this handler
	Name() string

	// Description returns a human-readable description of what this handler does
	Description() string

	// RunMode returns whether this handler runs once or many times
	RunMode() RunMode

	// Process takes a group of trigger matches and generates actions
	// The matches are grouped by pack and options before being passed here
	Process(matches []TriggerMatch) ([]Action, error)

	// ValidateOptions checks if the provided options are valid for this handler
	ValidateOptions(options map[string]interface{}) error

	// GetTemplateContent returns the template content for this handler
	// Returns empty string if the handler doesn't provide a template
	GetTemplateContent() string
}

// HandlerFactory is a function that creates a new Handler instance
// It takes a map of options to configure the handler
type HandlerFactory func(options map[string]interface{}) (Handler, error)
