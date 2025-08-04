package synthfs

import (
	"io/fs"

	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// DryRunFS is a filesystem wrapper that simulates operations without
// actually writing to the underlying filesystem.
type DryRunFS struct {
	memFS *filesystem.TestFileSystem
}

// NewDryRunFS creates a new DryRunFS.
func NewDryRunFS() *DryRunFS {
	return &DryRunFS{
		memFS: filesystem.NewTestFileSystem(),
	}
}

// Open opens the named file for reading.
func (fs *DryRunFS) Open(name string) (fs.File, error) {
	return fs.memFS.Open(name)
}

// Stat returns a FileInfo describing the named file.
func (fs *DryRunFS) Stat(name string) (fs.FileInfo, error) {
	return fs.memFS.Stat(name)
}

// ReadFile reads the file named by filename and returns the contents.
func (fs *DryRunFS) ReadFile(filename string) ([]byte, error) {
	return fs.memFS.ReadFile(filename)
}

// WriteFile writes data to a file named by filename.
func (fs *DryRunFS) WriteFile(filename string, data []byte, perm fs.FileMode) error {
	return fs.memFS.WriteFile(filename, data, perm)
}

// Mkdir creates a new directory with the specified name and permission bits.
func (fs *DryRunFS) Mkdir(name string, perm fs.FileMode) error {
	return fs.memFS.MkdirAll(name, perm)
}

// MkdirAll creates a directory named path,
// along with any necessary parents, and returns nil,
// or else returns an error.
func (fs *DryRunFS) MkdirAll(path string, perm fs.FileMode) error {
	return fs.memFS.MkdirAll(path, perm)
}

// Remove removes the named file or (empty) directory.
func (fs *DryRunFS) Remove(name string) error {
	return fs.memFS.Remove(name)
}

// RemoveAll removes path and any children it contains.
func (fs *DryRunFS) RemoveAll(path string) error {
	return fs.memFS.RemoveAll(path)
}

// Rename renames (moves) oldpath to newpath.
func (fs *DryRunFS) Rename(oldpath, newpath string) error {
	return fs.memFS.Rename(oldpath, newpath)
}

// Symlink creates a new symbolic link.
func (fs *DryRunFS) Symlink(oldname, newname string) error {
	return fs.memFS.Symlink(oldname, newname)
}

// Readlink returns the destination of the named symbolic link.
func (fs *DryRunFS) Readlink(name string) (string, error) {
	return fs.memFS.Readlink(name)
}
