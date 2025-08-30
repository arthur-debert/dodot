package types

// ClearedItem represents something that was removed during a clear operation
type ClearedItem struct {
	Type        string // "symlink", "brew_package", "script_output", etc.
	Path        string // What was removed/affected
	Description string // Human-readable description
}

// ClearContext provides all the resources needed for a handler to clean up
type ClearContext struct {
	Pack   Pack   // The pack being cleared
	FS     FS     // For file operations
	Paths  Pather // For path resolution
	DryRun bool   // Whether this is a dry run
}
