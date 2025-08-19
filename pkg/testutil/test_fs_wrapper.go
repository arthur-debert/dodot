package testutil

import (
	"io/fs"
	"path"
	"path/filepath"
	"sort"
	"strings"
	"testing/fstest"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// TestFS wraps filesystem.TestFileSystem to implement types.FS
type TestFS struct {
	*filesystem.TestFileSystem
}

// NewTestFS creates a new test filesystem that implements types.FS
func NewTestFS() types.FS {
	return &TestFS{
		TestFileSystem: filesystem.NewTestFileSystem(),
	}
}

// Lstat implements types.FS
// The underlying TestFileSystem properly supports symlinks via fstest.MapFS
func (t *TestFS) Lstat(name string) (fs.FileInfo, error) {
	// Check if it's a symlink by looking at the file mode
	if file, ok := t.MapFS[name]; ok && file.Mode&fs.ModeSymlink != 0 {
		// Return symlink info without following it
		return &mapFileInfo{
			name: filepath.Base(name),
			file: file,
		}, nil
	}
	// Not a symlink or doesn't exist, use regular Stat
	return t.Stat(name)
}

// ReadDir implements types.FS
func (t *TestFS) ReadDir(name string) ([]fs.DirEntry, error) {
	// First check if the directory exists
	info, err := t.Stat(name)
	if err != nil {
		return nil, err
	}
	if !info.IsDir() {
		return nil, &fs.PathError{Op: "readdir", Path: name, Err: fs.ErrInvalid}
	}

	// Get all files from the test filesystem
	// This is a simple implementation that works by iterating through
	// all stored paths and finding children of the given directory
	var entries []fs.DirEntry
	seen := make(map[string]bool)

	// The TestFileSystem is based on fstest.MapFS
	// We can iterate through the map keys to find children
	for filePath := range t.MapFS {
		// Check if this file is a direct child of our directory
		if isDirectChild(name, filePath) {
			// Extract the child name
			childName := getChildName(name, filePath)
			if seen[childName] {
				continue
			}
			seen[childName] = true

			// Get file info to create DirEntry
			childPath := joinPath(name, childName)
			info, err := t.Stat(childPath)
			if err != nil {
				continue
			}

			entries = append(entries, &testDirEntry{
				name: childName,
				info: info,
			})
		}
	}

	// Sort entries by name for consistent ordering
	sort.Slice(entries, func(i, j int) bool {
		return entries[i].Name() < entries[j].Name()
	})

	return entries, nil
}

// testDirEntry implements fs.DirEntry for test filesystem entries
type testDirEntry struct {
	name string
	info fs.FileInfo
}

func (e *testDirEntry) Name() string               { return e.name }
func (e *testDirEntry) IsDir() bool                { return e.info.IsDir() }
func (e *testDirEntry) Type() fs.FileMode          { return e.info.Mode().Type() }
func (e *testDirEntry) Info() (fs.FileInfo, error) { return e.info, nil }

// isDirectChild checks if filePath is a direct child of dir
func isDirectChild(dir, filePath string) bool {
	// Clean paths for comparison
	dir = filepath.Clean(dir)
	filePath = filepath.Clean(filePath)

	// Special case for root directory
	if dir == "." || dir == "/" {
		// Count slashes to determine if it's a direct child
		return !strings.Contains(filePath, string(filepath.Separator))
	}

	// Check if filePath starts with dir
	if !strings.HasPrefix(filePath, dir+string(filepath.Separator)) {
		return false
	}

	// Remove dir prefix and leading separator
	rel := strings.TrimPrefix(filePath, dir+string(filepath.Separator))

	// If there's no separator in the remaining path, it's a direct child
	return !strings.Contains(rel, string(filepath.Separator))
}

// getChildName extracts the child name from a path relative to parent
func getChildName(parent, child string) string {
	parent = filepath.Clean(parent)
	child = filepath.Clean(child)

	if parent == "." || parent == "/" {
		// For root, return the first component
		parts := strings.Split(child, string(filepath.Separator))
		if len(parts) > 0 {
			return parts[0]
		}
		return child
	}

	// Remove parent prefix
	rel := strings.TrimPrefix(child, parent+string(filepath.Separator))

	// Get first component
	parts := strings.Split(rel, string(filepath.Separator))
	if len(parts) > 0 {
		return parts[0]
	}
	return rel
}

// joinPath joins paths, handling special cases
func joinPath(dir, name string) string {
	if dir == "." {
		return name
	}
	return path.Join(dir, name)
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
