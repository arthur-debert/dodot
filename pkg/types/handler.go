package types

// RunMode indicates how often a power-up should be executed
type RunMode string

const (
	// RunModeMany indicates the power-up can be run multiple times safely
	RunModeMany RunMode = "many"

	// RunModeOnce indicates the power-up should only run once per pack
	RunModeOnce RunMode = "once"
)

const (
	// OverridePriority is a high priority value for config overrides
	OverridePriority = 100
)

// Handler is an interface for action generators that process matched files.
// Handlers receive groups of files that their associated triggers matched,
// and generate high-level actions describing what should be done.
type Handler interface {
	// Name returns the unique name of this power-up
	Name() string

	// Description returns a human-readable description of what this power-up does
	Description() string

	// RunMode returns whether this power-up runs once or many times
	RunMode() RunMode

	// Process takes a group of trigger matches and generates actions
	// The matches are grouped by pack and options before being passed here
	Process(matches []TriggerMatch) ([]Action, error)

	// ValidateOptions checks if the provided options are valid for this power-up
	ValidateOptions(options map[string]interface{}) error

	// GetTemplateContent returns the template content for this power-up
	// Returns empty string if the power-up doesn't provide a template
	GetTemplateContent() string
}

// HandlerFactory is a function that creates a new Handler instance
// It takes a map of options to configure the power-up
type HandlerFactory func(options map[string]interface{}) (Handler, error)
