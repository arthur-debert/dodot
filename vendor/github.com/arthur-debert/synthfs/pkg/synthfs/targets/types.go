// Package targets contains implementations of filesystem items.
package targets

// ItemType constants define the string representations of item types.
const (
	// ItemTypeFile is the string representation of a file item.
	ItemTypeFile = "file"
	// ItemTypeDirectory is the string representation of a directory item.
	ItemTypeDirectory = "directory"
	// ItemTypeSymlink is the string representation of a symlink item.
	ItemTypeSymlink = "symlink"
	// ItemTypeArchive is the string representation of an archive item.
	ItemTypeArchive = "archive"
	// ItemTypeUnarchive is the string representation of an unarchive item.
	ItemTypeUnarchive = "unarchive"
)

// Default values for file modes
const (
	// DefaultFileMode is the default mode for files (0644)
	DefaultFileMode = 0644
	// DefaultDirMode is the default mode for directories (0755)
	DefaultDirMode = 0755
)
