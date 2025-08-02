package targets

// FsItem defines the interface for a filesystem item in synthfs.
// It provides a common way to interact with different types of items like
// files, directories, and symlinks.
type FsItem interface {
	// Path returns the absolute path of the filesystem item.
	Path() string

	// Type returns a string representation of the item's type,
	// e.g., "file", "directory".
	Type() string
}
