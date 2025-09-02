package pipeline

// RuleMatch represents a successful rule match on a file or directory
// This is the output from the rules system that gets passed to handlers
type RuleMatch struct {
	// RuleName is the pattern that matched this file
	RuleName string

	// Pack is the name of the pack containing the matched file
	Pack string

	// Path is the relative path within the pack
	Path string

	// AbsolutePath is the absolute path to the file
	AbsolutePath string

	// Metadata contains any additional data about the matched file
	Metadata map[string]interface{}

	// HandlerName is the name of the handler that should process this match
	HandlerName string

	// HandlerOptions contains options to pass to the handler
	HandlerOptions map[string]interface{}

	// Priority determines the order of processing (higher = processed first)
	Priority int
}
