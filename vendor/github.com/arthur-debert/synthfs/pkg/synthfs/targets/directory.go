package targets

import (
	"io/fs"
)

// DirectoryItem represents a directory to be created.
// It holds the directory's path and permission mode.
type DirectoryItem struct {
	path string
	mode fs.FileMode
}

// NewDirectory creates a new DirectoryItem with default permissions.
// The path is the absolute path for the directory.
func NewDirectory(path string) *DirectoryItem {
	return &DirectoryItem{
		path: path,
		mode: 0755, // Default mode for directories
	}
}

// Path returns the directory's path.
func (di *DirectoryItem) Path() string {
	return di.path
}

// Type returns the string "directory".
func (di *DirectoryItem) Type() string {
	return "directory"
}

// WithMode sets the directory's permission mode.
func (di *DirectoryItem) WithMode(mode fs.FileMode) *DirectoryItem {
	di.mode = mode
	return di
}

// Mode returns the directory's permission mode.
func (di *DirectoryItem) Mode() fs.FileMode {
	return di.mode
}
