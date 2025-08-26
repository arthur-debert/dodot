package types

// ClearedItem represents something that was removed during a clear operation
type ClearedItem struct {
	Type        string // "symlink", "brew_package", "script_output", etc.
	Path        string // What was removed/affected
	Description string // Human-readable description
}

// PathResolver is the interface for path resolution
// This is a subset of paths.Paths to avoid circular imports
type PathResolver interface {
	PackHandlerDir(packName, handlerName string) string
	MapPackFileToSystem(pack *Pack, relPath string) string
}

// DataStoreInterface provides the minimal interface needed for clearing operations
type DataStoreInterface interface {
	DeleteProvisioningState(packName, handlerName string) error
}

// ClearContext provides all the resources needed for a handler to clean up
type ClearContext struct {
	Pack      Pack               // The pack being cleared
	DataStore DataStoreInterface // For accessing state information
	FS        FS                 // For file operations
	Paths     PathResolver       // For path resolution
	DryRun    bool               // Whether this is a dry run
}
