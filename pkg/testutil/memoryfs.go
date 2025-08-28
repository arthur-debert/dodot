package testutil

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

// MemoryFS implements types.FS interface with in-memory storage
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

// NewMemoryFS creates a new in-memory filesystem
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

// normalizePath converts a path to absolute form
func (m *MemoryFS) normalizePath(path string) string {
	if !filepath.IsAbs(path) {
		path = filepath.Join(m.cwd, path)
	}
	return filepath.Clean(path)
}

// getNode retrieves a node at the given path
func (m *MemoryFS) getNode(path string) (*fileNode, error) {
	path = m.normalizePath(path)

	// Check for injected errors
	if err, ok := m.errorPaths[path]; ok {
		return nil, err
	}

	node, exists := m.files[path]
	if !exists {
		return nil, &fs.PathError{Op: "open", Path: path, Err: fs.ErrNotExist}
	}

	return node, nil
}

// getParentAndName splits a path into parent directory and filename
func (m *MemoryFS) getParentAndName(path string) (parent *fileNode, name string, err error) {
	path = m.normalizePath(path)
	dir := filepath.Dir(path)
	name = filepath.Base(path)

	parent, err = m.getNode(dir)
	if err != nil {
		return nil, "", err
	}

	if !parent.isDir {
		return nil, "", &fs.PathError{Op: "open", Path: dir, Err: errors.New("not a directory")}
	}

	return parent, name, nil
}

// Open opens a file for reading
func (m *MemoryFS) Open(name string) (fs.File, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	m.readCount++

	node, err := m.getNode(name)
	if err != nil {
		return nil, err
	}

	// Follow symlink if needed
	if node.isLink {
		target := node.linkDest
		if !filepath.IsAbs(target) {
			target = filepath.Join(filepath.Dir(name), target)
		}
		node, err = m.getNode(target)
		if err != nil {
			return nil, err
		}
	}

	return &memoryFile{
		node:   node,
		reader: bytes.NewReader(node.content),
		fs:     m,
		path:   name,
	}, nil
}

// ReadFile reads the entire file content
func (m *MemoryFS) ReadFile(name string) ([]byte, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	m.readCount++

	node, err := m.getNode(name)
	if err != nil {
		return nil, err
	}

	if node.isDir {
		return nil, &fs.PathError{Op: "read", Path: name, Err: errors.New("is a directory")}
	}

	// Follow symlink
	if node.isLink {
		target := node.linkDest
		if !filepath.IsAbs(target) {
			target = filepath.Join(filepath.Dir(name), target)
		}
		node, err = m.getNode(target)
		if err != nil {
			return nil, err
		}
	}

	// Return a copy to prevent mutation
	content := make([]byte, len(node.content))
	copy(content, node.content)
	return content, nil
}

// WriteFile writes data to a file, creating it if necessary
func (m *MemoryFS) WriteFile(name string, data []byte, perm os.FileMode) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.writeCount++

	path := m.normalizePath(name)

	// Check for injected errors
	if err, ok := m.errorPaths[path]; ok {
		return err
	}

	parent, filename, err := m.getParentAndName(path)
	if err != nil {
		// Create parent directories if they don't exist
		if errors.Is(err, fs.ErrNotExist) {
			if err := m.mkdirAll(filepath.Dir(path), 0755); err != nil {
				return err
			}
			parent, filename, err = m.getParentAndName(path)
			if err != nil {
				return err
			}
		} else {
			return err
		}
	}

	// Create or update the file
	node := &fileNode{
		name:    filename,
		mode:    perm &^ m.umask,
		modTime: time.Now(),
		content: make([]byte, len(data)),
		isDir:   false,
	}
	copy(node.content, data)

	parent.children[filename] = node
	m.files[path] = node

	return nil
}

// Stat returns file info
func (m *MemoryFS) Stat(name string) (os.FileInfo, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	node, err := m.getNode(name)
	if err != nil {
		return nil, err
	}

	return &fileInfo{node: node, name: filepath.Base(name)}, nil
}

// Remove removes a file or empty directory
func (m *MemoryFS) Remove(name string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	path := m.normalizePath(name)

	node, err := m.getNode(path)
	if err != nil {
		return err
	}

	// Can't remove non-empty directory
	if node.isDir && len(node.children) > 0 {
		return &fs.PathError{Op: "remove", Path: name, Err: errors.New("directory not empty")}
	}

	// Remove from parent
	parent, filename, err := m.getParentAndName(path)
	if err != nil {
		return err
	}

	delete(parent.children, filename)
	delete(m.files, path)

	return nil
}

// RemoveAll removes a file or directory recursively
func (m *MemoryFS) RemoveAll(path string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	path = m.normalizePath(path)

	// Remove all descendants first
	toRemove := []string{}
	for p := range m.files {
		if strings.HasPrefix(p, path+"/") || p == path {
			toRemove = append(toRemove, p)
		}
	}

	// Remove from maps
	for _, p := range toRemove {
		delete(m.files, p)

		// Remove from parent's children
		if dir := filepath.Dir(p); dir != p {
			if parent, ok := m.files[dir]; ok && parent.isDir {
				delete(parent.children, filepath.Base(p))
			}
		}
	}

	return nil
}

// MkdirAll creates a directory and all necessary parents
func (m *MemoryFS) MkdirAll(path string, perm os.FileMode) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	return m.mkdirAll(path, perm)
}

// mkdirAll is the internal implementation without locking
func (m *MemoryFS) mkdirAll(path string, perm os.FileMode) error {
	path = m.normalizePath(path)

	// Check if already exists
	if node, err := m.getNode(path); err == nil {
		if !node.isDir {
			return &fs.PathError{Op: "mkdir", Path: path, Err: errors.New("file exists")}
		}
		return nil
	}

	// Create all parents
	parts := strings.Split(path, "/")
	current := "/"
	currentNode := m.files["/"]

	for i := 1; i < len(parts); i++ {
		if parts[i] == "" {
			continue
		}

		next := filepath.Join(current, parts[i])

		if child, exists := currentNode.children[parts[i]]; exists {
			if !child.isDir {
				return &fs.PathError{Op: "mkdir", Path: next, Err: errors.New("not a directory")}
			}
			currentNode = child
			current = next
			continue
		}

		// Create new directory
		newDir := &fileNode{
			name:     parts[i],
			mode:     perm | os.ModeDir,
			modTime:  time.Now(),
			isDir:    true,
			children: make(map[string]*fileNode),
		}

		currentNode.children[parts[i]] = newDir
		m.files[next] = newDir

		currentNode = newDir
		current = next
	}

	return nil
}

// Readlink returns the destination of a symbolic link
func (m *MemoryFS) Readlink(name string) (string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	node, err := m.getNode(name)
	if err != nil {
		return "", err
	}

	if !node.isLink {
		return "", &fs.PathError{Op: "readlink", Path: name, Err: errors.New("not a symbolic link")}
	}

	return node.linkDest, nil
}

// Symlink creates a symbolic link
func (m *MemoryFS) Symlink(target, link string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	linkPath := m.normalizePath(link)

	// Check if link already exists
	if _, err := m.getNode(linkPath); err == nil {
		return &fs.PathError{Op: "symlink", Path: link, Err: os.ErrExist}
	}

	parent, filename, err := m.getParentAndName(linkPath)
	if err != nil {
		return err
	}

	// Create symlink node
	node := &fileNode{
		name:     filename,
		mode:     0777 | os.ModeSymlink,
		modTime:  time.Now(),
		isLink:   true,
		linkDest: target,
	}

	parent.children[filename] = node
	m.files[linkPath] = node

	return nil
}

// ReadDir reads a directory and returns its entries
func (m *MemoryFS) ReadDir(name string) ([]fs.DirEntry, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	node, err := m.getNode(name)
	if err != nil {
		return nil, err
	}

	if !node.isDir {
		return nil, &fs.PathError{Op: "readdir", Path: name, Err: errors.New("not a directory")}
	}

	entries := make([]fs.DirEntry, 0, len(node.children))
	for childName, child := range node.children {
		entries = append(entries, &dirEntry{
			name: childName,
			info: &fileInfo{node: child, name: childName},
		})
	}

	return entries, nil
}

// Lstat returns file info without following symlinks
func (m *MemoryFS) Lstat(name string) (os.FileInfo, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	// For MemoryFS, Lstat behaves the same as Stat since we track symlinks explicitly
	node, err := m.getNode(name)
	if err != nil {
		return nil, err
	}

	return &fileInfo{node: node, name: filepath.Base(name)}, nil
}

// Getwd returns the current working directory
func (m *MemoryFS) Getwd() (string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.cwd, nil
}

// Chdir changes the current working directory
func (m *MemoryFS) Chdir(dir string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	path := m.normalizePath(dir)
	node, err := m.getNode(path)
	if err != nil {
		return err
	}

	if !node.isDir {
		return &fs.PathError{Op: "chdir", Path: dir, Err: errors.New("not a directory")}
	}

	m.cwd = path
	return nil
}

// WithError configures the filesystem to return an error for a specific path
func (m *MemoryFS) WithError(path string, err error) *MemoryFS {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.errorPaths[m.normalizePath(path)] = err
	return m
}

// Stats returns filesystem operation statistics
func (m *MemoryFS) Stats() (reads, writes int) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.readCount, m.writeCount
}

// memoryFile implements fs.File
type memoryFile struct {
	node   *fileNode
	reader *bytes.Reader
	fs     *MemoryFS
	path   string
}

func (f *memoryFile) Stat() (os.FileInfo, error) {
	return &fileInfo{node: f.node, name: f.node.name}, nil
}

func (f *memoryFile) Read(b []byte) (int, error) {
	if f.node.isDir {
		return 0, &fs.PathError{Op: "read", Path: f.path, Err: errors.New("is a directory")}
	}
	return f.reader.Read(b)
}

func (f *memoryFile) Close() error {
	return nil
}

// ReadDir implements fs.ReadDirFile
func (f *memoryFile) ReadDir(n int) ([]fs.DirEntry, error) {
	if !f.node.isDir {
		return nil, &fs.PathError{Op: "readdir", Path: f.path, Err: errors.New("not a directory")}
	}

	entries := make([]fs.DirEntry, 0, len(f.node.children))
	for name, child := range f.node.children {
		entries = append(entries, &dirEntry{
			name: name,
			info: &fileInfo{node: child, name: name},
		})
	}

	if n <= 0 {
		return entries, nil
	}

	if n > len(entries) {
		n = len(entries)
	}
	return entries[:n], nil
}

// fileInfo implements os.FileInfo
type fileInfo struct {
	node *fileNode
	name string
}

func (fi *fileInfo) Name() string       { return fi.name }
func (fi *fileInfo) Size() int64        { return int64(len(fi.node.content)) }
func (fi *fileInfo) Mode() os.FileMode  { return fi.node.mode }
func (fi *fileInfo) ModTime() time.Time { return fi.node.modTime }
func (fi *fileInfo) IsDir() bool        { return fi.node.isDir }
func (fi *fileInfo) Sys() interface{}   { return fi.node }

// dirEntry implements fs.DirEntry
type dirEntry struct {
	name string
	info os.FileInfo
}

func (de *dirEntry) Name() string               { return de.name }
func (de *dirEntry) IsDir() bool                { return de.info.IsDir() }
func (de *dirEntry) Type() os.FileMode          { return de.info.Mode().Type() }
func (de *dirEntry) Info() (os.FileInfo, error) { return de.info, nil }
