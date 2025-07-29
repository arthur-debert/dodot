package types

import "io/fs"

// Trigger is an interface for pattern-matching engines that scan files
// and directories within packs. When a trigger finds a match, it returns
// metadata about what was found.
type Trigger interface {
	// Name returns the unique name of this trigger
	Name() string

	// Description returns a human-readable description of what this trigger matches
	Description() string

	// Match checks if the given file or directory matches this trigger's pattern
	// It returns true if the file matches, along with any extracted metadata
	Match(path string, info fs.FileInfo) (bool, map[string]interface{})

	// Priority returns the priority of this trigger (higher = evaluated first)
	Priority() int
}

// TriggerMatch represents a successful trigger match on a file or directory
type TriggerMatch struct {
	// TriggerName is the name of the trigger that matched
	TriggerName string

	// Pack is the name of the pack containing the matched file
	Pack string

	// Path is the relative path within the pack
	Path string

	// AbsolutePath is the absolute path to the file
	AbsolutePath string

	// Metadata contains any additional data extracted by the trigger
	Metadata map[string]interface{}

	// PowerUpName is the name of the power-up that should process this match
	PowerUpName string

	// PowerUpOptions contains options to pass to the power-up
	PowerUpOptions map[string]interface{}

	// Priority determines the order of processing (higher = processed first)
	Priority int
}

// TriggerFactory creates a new Trigger instance with the given options
type TriggerFactory func(options map[string]interface{}) (Trigger, error)
