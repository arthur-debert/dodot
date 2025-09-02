package datastore

import (
	"bytes"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// MemoryFS implements types.FS interface with in-memory storage for testing
type MemoryFS struct {
	mu    sync.RWMutex
	files map[string]*fileNode
	cwd   string
	umask os.FileMode

	// Error injection
	errorPaths map[string]error

	// Statistics
	readCount  int
	writeCount int
}

// fileNode represents a file or directory in memory
type fileNode struct {
	name     string
	mode     os.FileMode
	modTime  time.Time
	content  []byte
	isDir    bool
	isLink   bool
	linkDest string
	children map[string]*fileNode
}

// NewMemoryFS creates a new in-memory filesystem for testing
func NewMemoryFS() *MemoryFS {
	root := &fileNode{
		name:     "/",
		mode:     0755 | os.ModeDir,
		modTime:  time.Now(),
		isDir:    true,
		children: make(map[string]*fileNode),
	}

	return &MemoryFS{
		files:      map[string]*fileNode{"/": root},
		cwd:        "/",
		umask:      0022,
		errorPaths: make(map[string]error),
	}
}

// Helper methods

func (m *MemoryFS) normalizePath(path string) string {
	if !filepath.IsAbs(path) {
		path = filepath.Join(m.cwd, path)
	}
	return filepath.Clean(path)
}

func (m *MemoryFS) getNode(path string) (*fileNode, error) {
	path = m.normalizePath(path)

	if err, ok := m.errorPaths[path]; ok {
		return nil, err
	}

	parts := strings.Split(path, string(filepath.Separator))
	current := m.files["/"]

	for i := 1; i < len(parts); i++ {
		if parts[i] == "" {
			continue
		}

		if !current.isDir {
			return nil, os.ErrNotExist
		}

		next, exists := current.children[parts[i]]
		if !exists {
			return nil, os.ErrNotExist
		}

		current = next
	}

	return current, nil
}

func (m *MemoryFS) createNode(path string, isDir bool) (*fileNode, error) {
	path = m.normalizePath(path)
	dir := filepath.Dir(path)
	base := filepath.Base(path)

	parent, err := m.getNode(dir)
	if err != nil {
		return nil, err
	}

	if !parent.isDir {
		return nil, errors.New("parent is not a directory")
	}

	node := &fileNode{
		name:    base,
		mode:    0644,
		modTime: time.Now(),
		isDir:   isDir,
	}

	if isDir {
		node.mode = 0755 | os.ModeDir
		node.children = make(map[string]*fileNode)
	}

	parent.children[base] = node
	return node, nil
}

// Filesystem operations

func (m *MemoryFS) Stat(name string) (fs.FileInfo, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	node, err := m.getNode(name)
	if err != nil {
		return nil, err
	}

	return &fileInfo{node: node, name: filepath.Base(name)}, nil
}

func (m *MemoryFS) Lstat(name string) (fs.FileInfo, error) {
	// For memory FS, Lstat behaves the same as Stat
	return m.Stat(name)
}

func (m *MemoryFS) ReadFile(name string) ([]byte, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	m.readCount++

	node, err := m.getNode(name)
	if err != nil {
		return nil, err
	}

	if node.isDir {
		return nil, errors.New("is a directory")
	}

	return append([]byte(nil), node.content...), nil
}

func (m *MemoryFS) WriteFile(name string, data []byte, perm fs.FileMode) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.writeCount++

	node, err := m.getNode(name)
	if err == os.ErrNotExist {
		node, err = m.createNode(name, false)
		if err != nil {
			return err
		}
	} else if err != nil {
		return err
	}

	if node.isDir {
		return errors.New("is a directory")
	}

	node.content = append([]byte(nil), data...)
	node.mode = perm
	node.modTime = time.Now()
	return nil
}

func (m *MemoryFS) MkdirAll(path string, perm fs.FileMode) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	path = m.normalizePath(path)
	parts := strings.Split(path, string(filepath.Separator))
	current := m.files["/"]

	for i := 1; i < len(parts); i++ {
		if parts[i] == "" {
			continue
		}

		next, exists := current.children[parts[i]]
		if !exists {
			next = &fileNode{
				name:     parts[i],
				mode:     perm | os.ModeDir,
				modTime:  time.Now(),
				isDir:    true,
				children: make(map[string]*fileNode),
			}
			current.children[parts[i]] = next
		} else if !next.isDir {
			return errors.New("not a directory")
		}

		current = next
	}

	return nil
}

func (m *MemoryFS) ReadDir(name string) ([]fs.DirEntry, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	node, err := m.getNode(name)
	if err != nil {
		return nil, err
	}

	if !node.isDir {
		return nil, errors.New("not a directory")
	}

	entries := make([]fs.DirEntry, 0, len(node.children))
	for _, child := range node.children {
		entries = append(entries, &dirEntry{node: child})
	}

	return entries, nil
}

func (m *MemoryFS) Symlink(oldname, newname string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	// Create parent directory if needed
	dir := filepath.Dir(newname)
	if _, err := m.getNode(dir); err == os.ErrNotExist {
		if err := m.MkdirAll(dir, 0755); err != nil {
			return err
		}
	}

	node, err := m.createNode(newname, false)
	if err != nil {
		return err
	}

	node.isLink = true
	node.linkDest = oldname
	node.mode |= os.ModeSymlink
	return nil
}

func (m *MemoryFS) Readlink(name string) (string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	node, err := m.getNode(name)
	if err != nil {
		return "", err
	}

	if !node.isLink {
		return "", errors.New("not a symbolic link")
	}

	return node.linkDest, nil
}

func (m *MemoryFS) Remove(name string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	path := m.normalizePath(name)
	dir := filepath.Dir(path)
	base := filepath.Base(path)

	parent, err := m.getNode(dir)
	if err != nil {
		return err
	}

	if !parent.isDir {
		return errors.New("parent is not a directory")
	}

	node, exists := parent.children[base]
	if !exists {
		return os.ErrNotExist
	}

	if node.isDir && len(node.children) > 0 {
		return errors.New("directory not empty")
	}

	delete(parent.children, base)
	return nil
}

func (m *MemoryFS) RemoveAll(path string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	path = m.normalizePath(path)
	dir := filepath.Dir(path)
	base := filepath.Base(path)

	if path == "/" {
		// Clear root but don't remove it
		m.files["/"].children = make(map[string]*fileNode)
		return nil
	}

	parent, err := m.getNode(dir)
	if err != nil {
		return err
	}

	if !parent.isDir {
		return errors.New("parent is not a directory")
	}

	delete(parent.children, base)
	return nil
}

func (m *MemoryFS) Rename(oldpath, newpath string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	oldpath = m.normalizePath(oldpath)
	newpath = m.normalizePath(newpath)

	oldDir := filepath.Dir(oldpath)
	oldBase := filepath.Base(oldpath)
	newDir := filepath.Dir(newpath)
	newBase := filepath.Base(newpath)

	oldParent, err := m.getNode(oldDir)
	if err != nil {
		return err
	}

	node, exists := oldParent.children[oldBase]
	if !exists {
		return os.ErrNotExist
	}

	newParent, err := m.getNode(newDir)
	if err != nil {
		return err
	}

	delete(oldParent.children, oldBase)
	newParent.children[newBase] = node
	node.name = newBase

	return nil
}

// fileInfo implements fs.FileInfo
type fileInfo struct {
	node *fileNode
	name string
}

func (fi *fileInfo) Name() string       { return fi.name }
func (fi *fileInfo) Size() int64        { return int64(len(fi.node.content)) }
func (fi *fileInfo) Mode() fs.FileMode  { return fi.node.mode }
func (fi *fileInfo) ModTime() time.Time { return fi.node.modTime }
func (fi *fileInfo) IsDir() bool        { return fi.node.isDir }
func (fi *fileInfo) Sys() interface{}   { return nil }

// dirEntry implements fs.DirEntry
type dirEntry struct {
	node *fileNode
}

func (de *dirEntry) Name() string      { return de.node.name }
func (de *dirEntry) IsDir() bool       { return de.node.isDir }
func (de *dirEntry) Type() fs.FileMode { return de.node.mode.Type() }
func (de *dirEntry) Info() (fs.FileInfo, error) {
	return &fileInfo{node: de.node, name: de.node.name}, nil
}

// Test helper methods

// SetError makes the filesystem return an error for a specific path
func (m *MemoryFS) SetError(path string, err error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.errorPaths[m.normalizePath(path)] = err
}

// ClearErrors removes all error injections
func (m *MemoryFS) ClearErrors() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.errorPaths = make(map[string]error)
}

// GetReadCount returns the number of read operations
func (m *MemoryFS) GetReadCount() int {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.readCount
}

// GetWriteCount returns the number of write operations
func (m *MemoryFS) GetWriteCount() int {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.writeCount
}

// CreateFileWithContent is a helper to create a file with content
func (m *MemoryFS) CreateFileWithContent(path string, content string) error {
	return m.WriteFile(path, []byte(content), 0644)
}

// FileExists checks if a file exists
func (m *MemoryFS) FileExists(path string) bool {
	_, err := m.Stat(path)
	return err == nil
}

// IsSymlink checks if a path is a symlink
func (m *MemoryFS) IsSymlink(path string) bool {
	m.mu.RLock()
	defer m.mu.RUnlock()

	node, err := m.getNode(path)
	if err != nil {
		return false
	}
	return node.isLink
}

// GetLinkTarget returns the target of a symlink
func (m *MemoryFS) GetLinkTarget(path string) (string, error) {
	return m.Readlink(path)
}

// String returns a string representation of the filesystem tree
func (m *MemoryFS) String() string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	var buf bytes.Buffer
	m.printTree(&buf, m.files["/"], "", true)
	return buf.String()
}

func (m *MemoryFS) printTree(buf *bytes.Buffer, node *fileNode, prefix string, isLast bool) {
	if node.name != "/" {
		buf.WriteString(prefix)
		if isLast {
			buf.WriteString("└── ")
		} else {
			buf.WriteString("├── ")
		}
		buf.WriteString(node.name)
		if node.isDir {
			buf.WriteString("/")
		} else if node.isLink {
			buf.WriteString(" -> ")
			buf.WriteString(node.linkDest)
		}
		buf.WriteString("\n")
	}

	if node.isDir {
		children := make([]*fileNode, 0, len(node.children))
		for _, child := range node.children {
			children = append(children, child)
		}

		for i, child := range children {
			childPrefix := prefix
			if node.name != "/" {
				if isLast {
					childPrefix += "    "
				} else {
					childPrefix += "│   "
				}
			}
			m.printTree(buf, child, childPrefix, i == len(children)-1)
		}
	}
}
