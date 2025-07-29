package types

// Pack represents a directory containing dotfiles and configuration
type Pack struct {
	// Name is the pack name (usually the directory name)
	Name string
	
	// Path is the absolute path to the pack directory
	Path string
	
	// Description is an optional description of the pack
	Description string
	
	// Priority determines the order of pack processing (higher = processed first)
	Priority int
	
	// Config contains pack-specific configuration from .dodot.toml
	Config PackConfig
	
	// Metadata contains any additional pack information
	Metadata map[string]interface{}
}

// PackConfig represents configuration options for a pack
type PackConfig struct {
	// Description overrides the pack description
	Description string `toml:"description"`
	
	// Priority overrides the default pack priority
	Priority int `toml:"priority"`
	
	// Disabled indicates if this pack should be skipped
	Disabled bool `toml:"disabled"`
	
	// Matchers contains custom matcher configurations for this pack
	Matchers []MatcherConfig `toml:"matchers"`
	
	// PowerUpOptions contains pack-specific options for power-ups
	PowerUpOptions map[string]map[string]interface{} `toml:"powerup_options"`
}