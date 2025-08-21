package types

// Matcher connects triggers to handlers. It specifies:
// "when this trigger fires, invoke this handler with these options."
type Matcher struct {
	// Name is an optional name for this matcher
	Name string

	// TriggerName specifies which trigger to use
	TriggerName string

	// HandlerName specifies which handler to invoke on match
	HandlerName string

	// Priority determines the order of matcher evaluation (higher = first)
	Priority int

	// Options contains configuration for both trigger and handler
	Options map[string]interface{}

	// TriggerOptions contains trigger-specific options
	TriggerOptions map[string]interface{}

	// HandlerOptions contains handler-specific options
	HandlerOptions map[string]interface{}

	// Enabled indicates if this matcher is active
	Enabled bool
}

// MatcherConfig represents matcher configuration from TOML files
type MatcherConfig struct {
	// Name is an optional name for this matcher
	Name string `toml:"name"`

	// Trigger specifies which trigger to use
	Trigger string `toml:"trigger"`

	// Handler specifies which handler to invoke
	Handler string `toml:"handler"`

	// Priority for this matcher
	Priority int `toml:"priority"`

	// Pattern is a common trigger option (for convenience)
	Pattern string `toml:"pattern"`

	// Target is a common handler option (for convenience)
	Target string `toml:"target"`

	// Options for trigger and handler configuration
	Options map[string]interface{} `toml:"options"`

	// TriggerOptions for trigger-specific configuration
	TriggerOptions map[string]interface{} `toml:"trigger_options"`

	// HandlerOptions for handler-specific configuration
	HandlerOptions map[string]interface{} `toml:"handler_options"`

	// Enabled indicates if this matcher is active (default: true)
	Enabled *bool `toml:"enabled"`
}
