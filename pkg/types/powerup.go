package types

// RunMode indicates how often a power-up should be executed
type RunMode string

const (
	// RunModeMany indicates the power-up can be run multiple times safely
	RunModeMany RunMode = "many"
	
	// RunModeOnce indicates the power-up should only run once per pack
	RunModeOnce RunMode = "once"
)

// PowerUp is an interface for action generators that process matched files.
// PowerUps receive groups of files that their associated triggers matched,
// and generate high-level actions describing what should be done.
type PowerUp interface {
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
}

// PowerUpFactory is a function that creates a new PowerUp instance
// It takes a map of options to configure the power-up
type PowerUpFactory func(options map[string]interface{}) (PowerUp, error)
