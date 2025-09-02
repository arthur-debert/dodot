package pipeline

// Match represents a file matched by a rule
type Match struct {
	PackName    string                 // Name of the pack
	FilePath    string                 // Relative path within pack
	FileName    string                 // Base name of the file
	IsDirectory bool                   // Whether this is a directory
	Handler     string                 // Handler to process this file
	Options     map[string]interface{} // Handler options from rule
}

// ProcessorHandler runs before normal handlers to modify file list
type ProcessorHandler interface {
	// ProcessFiles modifies the file list before normal matching
	ProcessFiles(files []FileInfo, packPath string) ([]FileInfo, error)
}

// FileInfo represents a file or directory to be processed
type FileInfo struct {
	Path        string // Relative path within pack
	Name        string // Base name
	IsDirectory bool   // Whether this is a directory
}
