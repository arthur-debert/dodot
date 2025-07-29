package types

// Matcher connects triggers to power-ups. It specifies:
// "when this trigger fires, invoke this power-up with these options."
type Matcher struct {
	// Name is an optional name for this matcher
	Name string
	
	// TriggerName specifies which trigger to use
	TriggerName string
	
	// PowerUpName specifies which power-up to invoke on match
	PowerUpName string
	
	// Priority determines the order of matcher evaluation (higher = first)
	Priority int
	
	// Options contains configuration for both trigger and power-up
	Options map[string]interface{}
	
	// TriggerOptions contains trigger-specific options
	TriggerOptions map[string]interface{}
	
	// PowerUpOptions contains power-up-specific options
	PowerUpOptions map[string]interface{}
	
	// Enabled indicates if this matcher is active
	Enabled bool
}

// MatcherConfig represents matcher configuration from TOML files
type MatcherConfig struct {
	// Name is an optional name for this matcher
	Name string `toml:"name"`
	
	// Trigger specifies which trigger to use
	Trigger string `toml:"trigger"`
	
	// PowerUp specifies which power-up to invoke
	PowerUp string `toml:"powerup"`
	
	// Priority for this matcher
	Priority int `toml:"priority"`
	
	// Pattern is a common trigger option (for convenience)
	Pattern string `toml:"pattern"`
	
	// Target is a common power-up option (for convenience)
	Target string `toml:"target"`
	
	// Options for trigger and power-up configuration
	Options map[string]interface{} `toml:"options"`
	
	// TriggerOptions for trigger-specific configuration
	TriggerOptions map[string]interface{} `toml:"trigger_options"`
	
	// PowerUpOptions for power-up-specific configuration
	PowerUpOptions map[string]interface{} `toml:"powerup_options"`
	
	// Enabled indicates if this matcher is active (default: true)
	Enabled *bool `toml:"enabled"`
}