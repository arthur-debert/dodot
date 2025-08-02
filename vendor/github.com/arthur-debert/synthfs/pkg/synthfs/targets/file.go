// Package targets provides implementations of file system items.
package targets

import (
	"io/fs"
)

// FileItem represents a file to be created in the synthetic filesystem.
// It holds the file's path, content, and permissions.
type FileItem struct {
	path    string
	content []byte
	mode    fs.FileMode
}

// NewFile creates a new FileItem with default permissions.
// The path is the absolute path for the file.
func NewFile(path string) *FileItem {
	return &FileItem{
		path: path,
		mode: 0644, // Default mode for files
	}
}

// Path returns the file's path.
func (fi *FileItem) Path() string {
	return fi.path
}

// Type returns the string "file".
func (fi *FileItem) Type() string {
	return "file"
}

// WithContent sets the byte content for the file.
func (fi *FileItem) WithContent(content []byte) *FileItem {
	fi.content = content
	return fi
}

// Content returns the file's byte content.
func (fi *FileItem) Content() []byte {
	return fi.content
}

// WithMode sets the file's permission mode.
func (fi *FileItem) WithMode(mode fs.FileMode) *FileItem {
	fi.mode = mode
	return fi
}

// Mode returns the file's permission mode.
func (fi *FileItem) Mode() fs.FileMode {
	return fi.mode
}
