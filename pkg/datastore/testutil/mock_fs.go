package testutil

import (
	"io/fs"
	"path/filepath"
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

// Stat implements types.FS
func (m *MockFS) Stat(name string) (fs.FileInfo, error) {
	return m.MapFS.Stat(name)
}

// ReadFile implements types.FS
func (m *MockFS) ReadFile(name string) ([]byte, error) {
	file, ok := m.MapFS[name]
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
	m.MapFS[name] = &fstest.MapFile{
		Data:    data,
		Mode:    perm,
		ModTime: time.Now(),
	}
	return nil
}

// MkdirAll implements types.FS
func (m *MockFS) MkdirAll(path string, perm fs.FileMode) error {
	dir := path
	for {
		m.MapFS[dir] = &fstest.MapFile{
			Mode:    fs.ModeDir | perm,
			ModTime: time.Now(),
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	return nil
}

// Symlink implements types.FS
func (m *MockFS) Symlink(oldname, newname string) error {
	m.MapFS[newname] = &fstest.MapFile{
		Data:    []byte(oldname),
		Mode:    fs.ModeSymlink,
		ModTime: time.Now(),
	}
	return nil
}

// Readlink implements types.FS
func (m *MockFS) Readlink(name string) (string, error) {
	file, ok := m.MapFS[name]
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
	delete(m.MapFS, name)
	return nil
}

// RemoveAll implements types.FS
func (m *MockFS) RemoveAll(path string) error {
	prefix := path
	for p := range m.MapFS {
		if strings.HasPrefix(p, prefix) {
			delete(m.MapFS, p)
		}
	}
	return nil
}

// Lstat implements types.FS
func (m *MockFS) Lstat(name string) (fs.FileInfo, error) {
	file, ok := m.MapFS[name]
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
	var entries []fs.DirEntry
	dir := name
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
