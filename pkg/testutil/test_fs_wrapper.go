package testutil

import (
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
	"io/fs"
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
// For testing, we treat Lstat the same as Stat since TestFileSystem
// doesn't distinguish between regular files and symlinks
func (t *TestFS) Lstat(name string) (fs.FileInfo, error) {
	return t.Stat(name)
}
