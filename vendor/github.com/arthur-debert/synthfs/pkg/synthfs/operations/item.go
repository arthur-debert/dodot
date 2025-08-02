package operations

import "io/fs"

// MinimalItem is a minimal implementation of the item interface for reverse operations
type MinimalItem struct {
	path     string
	itemType string
	content  []byte
	mode     fs.FileMode
}

func (m *MinimalItem) Path() string {
	return m.path
}

func (m *MinimalItem) Type() string {
	return m.itemType
}

func (m *MinimalItem) Content() []byte {
	return m.content
}

func (m *MinimalItem) Mode() fs.FileMode {
	return m.mode
}
