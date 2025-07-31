package filesystem

import (
	"context"
	"io/fs"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"testing/fstest"
)

// TestFileSystem extends fstest.MapFS to implement our FileSystem interface
// This allows using testing/fstest with synthfs operations
type TestFileSystem struct {
	fstest.MapFS
}

// NewTestFileSystem creates a new test filesystem based on fstest.MapFS
func NewTestFileSystem() *TestFileSystem {
	return &TestFileSystem{
		MapFS: make(fstest.MapFS),
	}
}

// NewTestFileSystemFromMap creates a test filesystem from an existing map
func NewTestFileSystemFromMap(files map[string]*fstest.MapFile) *TestFileSystem {
	return &TestFileSystem{
		MapFS: files,
	}
}

// WriteFile implements WriteFS for testing
func (tfs *TestFileSystem) WriteFile(name string, data []byte, perm fs.FileMode) error {
	if !fs.ValidPath(name) {
		return &fs.PathError{Op: "writefile", Path: name, Err: fs.ErrInvalid}
	}
	tfs.MapFS[name] = &fstest.MapFile{
		Data: data,
		Mode: perm,
	}
	return nil
}

// MkdirAll implements WriteFS for testing
func (tfs *TestFileSystem) MkdirAll(path string, perm fs.FileMode) error {
	if !fs.ValidPath(path) {
		return &fs.PathError{Op: "mkdirall", Path: path, Err: fs.ErrInvalid}
	}
	// For testing, we'll create a directory entry
	tfs.MapFS[path] = &fstest.MapFile{
		Mode: perm | fs.ModeDir,
	}
	return nil
}

// Remove implements WriteFS for testing
func (tfs *TestFileSystem) Remove(name string) error {
	if !fs.ValidPath(name) {
		return &fs.PathError{Op: "remove", Path: name, Err: fs.ErrInvalid}
	}
	if _, exists := tfs.MapFS[name]; !exists {
		return &fs.PathError{Op: "remove", Path: name, Err: fs.ErrNotExist}
	}
	delete(tfs.MapFS, name)
	return nil
}

// RemoveAll implements WriteFS for testing
func (tfs *TestFileSystem) RemoveAll(name string) error {
	if !fs.ValidPath(name) {
		return &fs.PathError{Op: "removeall", Path: name, Err: fs.ErrInvalid}
	}
	// Remove the path and all its children
	for path := range tfs.MapFS {
		if path == name || isSubPath(name, path) {
			delete(tfs.MapFS, path)
		}
	}
	return nil
}

// Symlink implements WriteFS for testing
func (tfs *TestFileSystem) Symlink(oldname, newname string) error {
	if !fs.ValidPath(newname) {
		return &fs.PathError{Op: "symlink", Path: newname, Err: fs.ErrInvalid}
	}

	targetPath := oldname
	if strings.Contains(oldname, "..") {
		// Handle relative paths for tests that need it
		dir := filepath.Dir(newname)
		targetPath = filepath.Clean(filepath.Join(dir, oldname))
	}

	// Check if target exists
	if _, exists := tfs.MapFS[targetPath]; !exists {
		// Unlike real symlinks, for testing we'll require the target to exist
		return &fs.PathError{Op: "symlink", Path: oldname, Err: fs.ErrNotExist}
	}

	// Check if newname already exists
	if _, exists := tfs.MapFS[newname]; exists {
		return &fs.PathError{Op: "symlink", Path: newname, Err: fs.ErrExist}
	}

	// Create symlink as a special file with ModeSymlink
	tfs.MapFS[newname] = &fstest.MapFile{
		Data: []byte(oldname), // Store target path as data
		Mode: fs.ModeSymlink | 0777,
	}
	return nil
}

// Readlink implements WriteFS for testing
func (tfs *TestFileSystem) Readlink(name string) (string, error) {
	if !fs.ValidPath(name) {
		return "", &fs.PathError{Op: "readlink", Path: name, Err: fs.ErrInvalid}
	}

	file, exists := tfs.MapFS[name]
	if !exists {
		return "", &fs.PathError{Op: "readlink", Path: name, Err: fs.ErrNotExist}
	}

	if file.Mode&fs.ModeSymlink == 0 {
		return "", &fs.PathError{Op: "readlink", Path: name, Err: syscall.EINVAL}
	}

	return string(file.Data), nil
}

// Rename implements WriteFS for testing
func (tfs *TestFileSystem) Rename(oldpath, newpath string) error {
	if !fs.ValidPath(oldpath) || !fs.ValidPath(newpath) {
		return &fs.PathError{Op: "rename", Path: newpath, Err: fs.ErrInvalid}
	}

	file, exists := tfs.MapFS[oldpath]
	if !exists {
		return &fs.PathError{Op: "rename", Path: oldpath, Err: fs.ErrNotExist}
	}

	// Check if newpath already exists
	if _, exists := tfs.MapFS[newpath]; exists {
		return &fs.PathError{Op: "rename", Path: newpath, Err: fs.ErrExist}
	}

	tfs.MapFS[newpath] = file
	delete(tfs.MapFS, oldpath)
	return nil
}

// Stat implements FullFileSystem for testing
func (tfs *TestFileSystem) Stat(name string) (fs.FileInfo, error) {
	if !fs.ValidPath(name) {
		return nil, &fs.PathError{Op: "stat", Path: name, Err: fs.ErrInvalid}
	}

	// Use Open to get the file and then get its info
	file, err := tfs.Open(name)
	if err != nil {
		return nil, err
	}
	defer func() {
		_ = file.Close() // Best effort close
	}()

	return file.Stat()
}

// isSubPath returns true if child is a subpath of parent
func isSubPath(parent, child string) bool {
	if parent == "" || parent == "." {
		return true
	}
	if child == parent {
		return true // The path itself should be included
	}
	if len(child) <= len(parent) {
		return false
	}
	return child[:len(parent)+1] == parent+"/"
}

// TestHelper provides utilities for testing synthfs operations
type TestHelper struct {
	t   *testing.T
	fs  *TestFileSystem
	ctx context.Context
}

// NewTestHelper creates a new test helper with a fresh filesystem
func NewTestHelper(t *testing.T) *TestHelper {
	return &TestHelper{
		t:   t,
		fs:  NewTestFileSystem(),
		ctx: context.Background(),
	}
}

// NewTestHelperWithFiles creates a test helper with predefined files
func NewTestHelperWithFiles(t *testing.T, files map[string]*fstest.MapFile) *TestHelper {
	return &TestHelper{
		t:   t,
		fs:  NewTestFileSystemFromMap(files),
		ctx: context.Background(),
	}
}

// FileSystem returns the test filesystem
func (th *TestHelper) FileSystem() *TestFileSystem {
	return th.fs
}

// Context returns the test context
func (th *TestHelper) Context() context.Context {
	return th.ctx
}

// WriteFile is a helper that writes a file and fails the test on error
func (th *TestHelper) WriteFile(name string, data []byte, perm fs.FileMode) {
	if err := th.fs.WriteFile(name, data, perm); err != nil {
		th.t.Fatalf("Failed to write file %s: %v", name, err)
	}
}

// MkdirAll is a helper that creates directories and fails the test on error
func (th *TestHelper) MkdirAll(path string, perm fs.FileMode) {
	if err := th.fs.MkdirAll(path, perm); err != nil {
		th.t.Fatalf("Failed to create directory %s: %v", path, err)
	}
}

// ReadFile is a helper that reads a file and fails the test on error
func (th *TestHelper) ReadFile(name string) []byte {
	file, err := th.fs.Open(name)
	if err != nil {
		th.t.Fatalf("Failed to open file %s: %v", name, err)
	}
	defer func() {
		_ = file.Close() // Best effort close
	}()

	info, err := file.Stat()
	if err != nil {
		th.t.Fatalf("Failed to stat file %s: %v", name, err)
	}

	data := make([]byte, info.Size())
	n, err := file.Read(data)
	if err != nil {
		th.t.Fatalf("Failed to read file %s: %v", name, err)
	}

	return data[:n]
}

// FileExists checks if a file exists
func (th *TestHelper) FileExists(name string) bool {
	_, err := th.fs.Stat(name)
	return err == nil
}

// AssertFileContent checks that a file has the expected content
func (th *TestHelper) AssertFileContent(name string, expected []byte) {
	actual := th.ReadFile(name)
	if string(actual) != string(expected) {
		th.t.Errorf("File %s content mismatch:\nExpected: %q\nActual: %q", name, expected, actual)
	}
}

// AssertFileNotExists checks that a file does not exist
func (th *TestHelper) AssertFileNotExists(name string) {
	if th.FileExists(name) {
		th.t.Errorf("Expected file %s to not exist, but it does", name)
	}
}