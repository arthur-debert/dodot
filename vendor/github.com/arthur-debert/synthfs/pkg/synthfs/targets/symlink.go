package targets

// SymlinkItem represents a symbolic link to be created.
// It holds the path of the link itself and the path of its target.
type SymlinkItem struct {
	path   string
	target string
}

// NewSymlink creates a new SymlinkItem.
// The path is the absolute path for the link, and target is what it points to.
func NewSymlink(path, target string) *SymlinkItem {
	return &SymlinkItem{
		path:   path,
		target: target,
	}
}

// Path returns the symlink's path.
func (si *SymlinkItem) Path() string {
	return si.path
}

// Type returns the string "symlink".
func (si *SymlinkItem) Type() string {
	return "symlink"
}

// Target returns the symlink's target path.
func (si *SymlinkItem) Target() string {
	return si.target
}
