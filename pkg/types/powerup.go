package types

// PowerUp is an interface for action generators that process matched files.
// PowerUps receive groups of files that their associated triggers matched,
// and generate high-level actions describing what should be done.
type PowerUp interface {
	// Name returns the unique name of this power-up
	Name() string
	
	// Description returns a human-readable description of what this power-up does
	Description() string
	
	// Process takes a group of trigger matches and generates actions
	// The matches are grouped by pack and options before being passed here
	Process(matches []TriggerMatch) ([]Action, error)
	
	// ValidateOptions checks if the provided options are valid for this power-up
	ValidateOptions(options map[string]interface{}) error
}