package filesystem

import (
	"io/fs"
)

// ReadFS is an alias for fs.FS, representing a read-only file system.
type ReadFS = fs.FS

// WriteFS defines the interface for write operations on a file system.
type WriteFS interface {
	WriteFile(name string, data []byte, perm fs.FileMode) error
	MkdirAll(path string, perm fs.FileMode) error
	Remove(name string) error
	RemoveAll(name string) error
	Symlink(oldname, newname string) error
	Readlink(name string) (string, error)
	Rename(oldpath, newpath string) error
}

// FileSystem combines read and write operations.
type FileSystem interface {
	ReadFS
	WriteFS
}

// StatFS extends ReadFS with Stat capabilities for better io/fs compatibility
type StatFS interface {
	ReadFS
	Stat(name string) (fs.FileInfo, error)
}

// FullFileSystem provides the complete filesystem interface including Stat
type FullFileSystem interface {
	FileSystem
	Stat(name string) (fs.FileInfo, error)
}
