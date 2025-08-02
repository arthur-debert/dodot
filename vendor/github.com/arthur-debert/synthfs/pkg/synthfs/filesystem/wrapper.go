package filesystem

import (
	"io/fs"
)

// ReadOnlyWrapper wraps an fs.FS to add StatFS capabilities if not already present
type ReadOnlyWrapper struct {
	fs.FS
}

// NewReadOnlyWrapper creates a new wrapper for an fs.FS
func NewReadOnlyWrapper(fsys fs.FS) *ReadOnlyWrapper {
	return &ReadOnlyWrapper{FS: fsys}
}

// Stat implements the StatFS interface
func (w *ReadOnlyWrapper) Stat(name string) (fs.FileInfo, error) {
	if statFS, ok := w.FS.(StatFS); ok {
		return statFS.Stat(name)
	}

	// Otherwise, open the file and get its info
	file, err := w.Open(name)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = file.Close()
	}()

	return file.Stat()
}
