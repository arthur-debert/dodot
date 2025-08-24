package testutil

import (
	"io/fs"
	"path/filepath"
	"runtime"
	"strings"
	"testing/fstest"
	"time"
)

// MockFS is a mock filesystem implementation for testing
// It's based on fstest.MapFS and provides a simple in-memory filesystem
type MockFS struct {
	fstest.MapFS
}

// NewMockFS creates a new mock filesystem.
func NewMockFS() *MockFS {
	return &MockFS{
		MapFS: make(fstest.MapFS),
	}
}

// normalizePath converts absolute paths to relative paths for MapFS storage
func (m *MockFS) normalizePath(path string) string {
	// Clean the path to handle both absolute and relative paths
	cleanPath := filepath.Clean(path)

	// Remove leading slash for absolute paths since MapFS uses relative paths
	if filepath.IsAbs(cleanPath) {
		// On Windows, we need to handle drive letters
		if runtime.GOOS == "windows" {
			// Remove drive letter and colon (e.g., "C:")
			if len(cleanPath) >= 2 && cleanPath[1] == ':' {
				cleanPath = cleanPath[2:]
			}
		}
		// Remove leading path separator
		cleanPath = strings.TrimPrefix(cleanPath, string(filepath.Separator))
	}

	return cleanPath
}

// Stat implements types.FS
func (m *MockFS) Stat(name string) (fs.FileInfo, error) {
	cleanPath := m.normalizePath(name)
	return m.MapFS.Stat(cleanPath)
}

// ReadFile implements types.FS
func (m *MockFS) ReadFile(name string) ([]byte, error) {
	cleanPath := m.normalizePath(name)
	file, ok := m.MapFS[cleanPath]
	if !ok {
		return nil, fs.ErrNotExist
	}
	if file.Mode.IsDir() {
		return nil, fs.ErrInvalid
	}
	return file.Data, nil
}

// WriteFile implements types.FS
func (m *MockFS) WriteFile(name string, data []byte, perm fs.FileMode) error {
	cleanPath := m.normalizePath(name)
	m.MapFS[cleanPath] = &fstest.MapFile{
		Data:    data,
		Mode:    perm,
		ModTime: time.Now(),
	}
	return nil
}

// MkdirAll implements types.FS
func (m *MockFS) MkdirAll(path string, perm fs.FileMode) error {
	cleanPath := m.normalizePath(path)
	dir := cleanPath
	for {
		m.MapFS[dir] = &fstest.MapFile{
			Mode:    fs.ModeDir | perm,
			ModTime: time.Now(),
		}
		parent := filepath.Dir(dir)
		if parent == dir || parent == "." {
			break
		}
		dir = parent
	}
	return nil
}

// Symlink implements types.FS
func (m *MockFS) Symlink(oldname, newname string) error {
	cleanPath := m.normalizePath(newname)
	m.MapFS[cleanPath] = &fstest.MapFile{
		Data:    []byte(oldname),
		Mode:    fs.ModeSymlink,
		ModTime: time.Now(),
	}
	return nil
}

// Readlink implements types.FS
func (m *MockFS) Readlink(name string) (string, error) {
	cleanPath := m.normalizePath(name)
	file, ok := m.MapFS[cleanPath]
	if !ok {
		return "", fs.ErrNotExist
	}
	if file.Mode&fs.ModeSymlink == 0 {
		return "", fs.ErrInvalid
	}
	return string(file.Data), nil
}

// Remove implements types.FS
func (m *MockFS) Remove(name string) error {
	cleanPath := m.normalizePath(name)
	delete(m.MapFS, cleanPath)
	return nil
}

// RemoveAll implements types.FS
func (m *MockFS) RemoveAll(path string) error {
	cleanPath := m.normalizePath(path)
	prefix := cleanPath

	// Remove the directory entry itself if it exists
	delete(m.MapFS, cleanPath)

	// Remove all entries with this prefix
	if !strings.HasSuffix(prefix, "/") {
		prefix = prefix + "/"
	}
	for p := range m.MapFS {
		if strings.HasPrefix(p, prefix) {
			delete(m.MapFS, p)
		}
	}
	return nil
}

// Lstat implements types.FS
func (m *MockFS) Lstat(name string) (fs.FileInfo, error) {
	cleanPath := m.normalizePath(name)
	file, ok := m.MapFS[cleanPath]
	if !ok {
		return nil, fs.ErrNotExist
	}
	return &mapFileInfo{
		name: filepath.Base(name),
		file: file,
	}, nil
}

// ReadDir implements types.FS
func (m *MockFS) ReadDir(name string) ([]fs.DirEntry, error) {
	cleanPath := m.normalizePath(name)
	var entries []fs.DirEntry
	dir := cleanPath
	for p, file := range m.MapFS {
		if filepath.Dir(p) == dir {
			entries = append(entries, &testDirEntry{
				name: filepath.Base(p),
				info: &mapFileInfo{
					name: filepath.Base(p),
					file: file,
				},
			})
		}
	}
	return entries, nil
}

// mapFileInfo implements fs.FileInfo for a MapFile
type mapFileInfo struct {
	name string
	file *fstest.MapFile
}

func (i *mapFileInfo) Name() string       { return i.name }
func (i *mapFileInfo) Size() int64        { return int64(len(i.file.Data)) }
func (i *mapFileInfo) Mode() fs.FileMode  { return i.file.Mode }
func (i *mapFileInfo) ModTime() time.Time { return i.file.ModTime }
func (i *mapFileInfo) IsDir() bool        { return i.file.Mode.IsDir() }
func (i *mapFileInfo) Sys() any           { return i.file.Sys }

// testDirEntry implements fs.DirEntry for test filesystem entries
type testDirEntry struct {
	name string
	info fs.FileInfo
}

func (e *testDirEntry) Name() string               { return e.name }
func (e *testDirEntry) IsDir() bool                { return e.info.IsDir() }
func (e *testDirEntry) Type() fs.FileMode          { return e.info.Mode().Type() }
func (e *testDirEntry) Info() (fs.FileInfo, error) { return e.info, nil }
