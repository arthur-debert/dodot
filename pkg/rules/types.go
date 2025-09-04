package rules

// Match represents a file matched by a rule
type Match struct {
	PackName    string                 // Name of the pack
	FilePath    string                 // Relative path within pack
	FileName    string                 // Base name of the file
	IsDirectory bool                   // Whether this is a directory
	Handler     string                 // Handler to process this file
	Options     map[string]interface{} // Handler options from rule
}

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

// FileInfo represents a file or directory to be processed
type FileInfo struct {
	Path        string // Relative path within pack
	Name        string // Base name
	IsDirectory bool   // Whether this is a directory
}

// ProcessorHandler runs before normal handlers to modify file list
type ProcessorHandler interface {
	// ProcessFiles modifies the file list before normal matching
	ProcessFiles(files []FileInfo, packPath string) ([]FileInfo, error)
}