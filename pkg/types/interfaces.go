package types

import (
	"io/fs"
)

// FS is the filesystem interface required for dodot operations
type FS interface {
	// File operations
	Stat(name string) (fs.FileInfo, error)
	ReadFile(name string) ([]byte, error)
	WriteFile(name string, data []byte, perm fs.FileMode) error

	// Directory operations
	MkdirAll(path string, perm fs.FileMode) error

	// Symlink operations
	Symlink(oldname, newname string) error
	Readlink(name string) (string, error)

	// Other operations
	Remove(name string) error
	RemoveAll(path string) error

	// Optional operations - implementations should check for support
	// For testing, Lstat can fall back to Stat
	Lstat(name string) (fs.FileInfo, error)
}

// Pather provides paths for dodot operations
type Pather interface {
	// DotfilesRoot returns the root directory for dotfiles
	DotfilesRoot() string

	// DataDir returns the XDG data directory for dodot
	DataDir() string

	// ConfigDir returns the XDG config directory for dodot
	ConfigDir() string

	// CacheDir returns the XDG cache directory for dodot
	CacheDir() string

	// StateDir returns the XDG state directory for dodot
	StateDir() string
}
