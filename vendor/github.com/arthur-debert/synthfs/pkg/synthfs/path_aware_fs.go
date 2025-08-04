package synthfs

import (
	"io/fs"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// NewOSFileSystemWithPaths creates an OS filesystem with path handling
func NewOSFileSystemWithPaths(root string) *PathAwareFileSystem {
	osfs := filesystem.NewOSFileSystem(root)
	return NewPathAwareFileSystem(osfs, root)
}

// NewTestFileSystemWithPaths creates a test filesystem with path handling
//
// DEPRECATED: Use real filesystem testing instead. TestFileSystem hides important
// security and behavioral issues. For new tests, use:
//
//	tempDir := t.TempDir()
//	osFS := filesystem.NewOSFileSystem(tempDir)
//	fs := NewPathAwareFileSystem(osFS, tempDir)
//
// Or use testutil.NewRealFSTestHelper(t) for a convenient helper.
// See docs/dev/README.md for testing guidelines.
func NewTestFileSystemWithPaths(root string) *PathAwareFileSystem {
	testfs := filesystem.NewTestFileSystem()
	return NewPathAwareFileSystem(testfs, root)
}

// PathAwareFileSystem wraps a FileSystem with intelligent path handling
type PathAwareFileSystem struct {
	fs      FileSystem
	handler *PathHandler
}

// NewPathAwareFileSystem creates a filesystem with path handling
func NewPathAwareFileSystem(fs FileSystem, base string) *PathAwareFileSystem {
	return &PathAwareFileSystem{
		fs:      fs,
		handler: NewPathHandler(base, PathModeAuto),
	}
}

// WithPathMode sets the path handling mode
func (pfs *PathAwareFileSystem) WithPathMode(mode PathMode) *PathAwareFileSystem {
	pfs.handler = NewPathHandler(pfs.handler.base, mode)
	return pfs
}

// WithAbsolutePaths forces absolute path handling
func (pfs *PathAwareFileSystem) WithAbsolutePaths() *PathAwareFileSystem {
	return pfs.WithPathMode(PathModeAbsolute)
}

// WithRelativePaths forces relative path handling
func (pfs *PathAwareFileSystem) WithRelativePaths() *PathAwareFileSystem {
	return pfs.WithPathMode(PathModeRelative)
}

// WithAutoDetectPaths enables automatic path detection (default)
func (pfs *PathAwareFileSystem) WithAutoDetectPaths() *PathAwareFileSystem {
	return pfs.WithPathMode(PathModeAuto)
}

// Open implements fs.FS
func (pfs *PathAwareFileSystem) Open(name string) (fs.File, error) {
	resolved, err := pfs.resolvePath(name)
	if err != nil {
		return nil, &fs.PathError{Op: "open", Path: name, Err: err}
	}
	return pfs.fs.Open(resolved)
}

// Stat implements StatFS
func (pfs *PathAwareFileSystem) Stat(name string) (fs.FileInfo, error) {
	resolved, err := pfs.resolvePath(name)
	if err != nil {
		return nil, &fs.PathError{Op: "stat", Path: name, Err: err}
	}

	// Check if the underlying FS implements StatFS
	if statFS, ok := pfs.fs.(filesystem.StatFS); ok {
		return statFS.Stat(resolved)
	}

	// Fallback to Open + Stat
	f, err := pfs.fs.Open(resolved)
	if err != nil {
		return nil, err
	}
	defer func() { _ = f.Close() }()
	return f.Stat()
}

// ReadFile implements ReadFS
func (pfs *PathAwareFileSystem) ReadFile(name string) ([]byte, error) {
	resolved, err := pfs.resolvePath(name)
	if err != nil {
		return nil, &fs.PathError{Op: "readfile", Path: name, Err: err}
	}

	f, err := pfs.fs.Open(resolved)
	if err != nil {
		return nil, err
	}
	defer func() { _ = f.Close() }()

	// Use fs.ReadFile if available
	return fs.ReadFile(pfs.fs, resolved)
}

// WriteFile implements WriteFS
func (pfs *PathAwareFileSystem) WriteFile(name string, data []byte, perm fs.FileMode) error {
	resolved, err := pfs.resolvePath(name)
	if err != nil {
		return &fs.PathError{Op: "writefile", Path: name, Err: err}
	}

	if writeFS, ok := pfs.fs.(filesystem.WriteFS); ok {
		return writeFS.WriteFile(resolved, data, perm)
	}

	return &fs.PathError{Op: "writefile", Path: name, Err: fs.ErrInvalid}
}

// MkdirAll implements WriteFS
func (pfs *PathAwareFileSystem) MkdirAll(path string, perm fs.FileMode) error {
	resolved, err := pfs.resolvePath(path)
	if err != nil {
		return &fs.PathError{Op: "mkdirall", Path: path, Err: err}
	}

	if writeFS, ok := pfs.fs.(filesystem.WriteFS); ok {
		return writeFS.MkdirAll(resolved, perm)
	}

	return &fs.PathError{Op: "mkdirall", Path: path, Err: fs.ErrInvalid}
}

// Remove implements WriteFS
func (pfs *PathAwareFileSystem) Remove(name string) error {
	resolved, err := pfs.resolvePath(name)
	if err != nil {
		return &fs.PathError{Op: "remove", Path: name, Err: err}
	}

	if writeFS, ok := pfs.fs.(filesystem.WriteFS); ok {
		return writeFS.Remove(resolved)
	}

	return &fs.PathError{Op: "remove", Path: name, Err: fs.ErrInvalid}
}

// RemoveAll implements WriteFS
func (pfs *PathAwareFileSystem) RemoveAll(name string) error {
	resolved, err := pfs.resolvePath(name)
	if err != nil {
		return &fs.PathError{Op: "removeall", Path: name, Err: err}
	}

	if writeFS, ok := pfs.fs.(filesystem.WriteFS); ok {
		return writeFS.RemoveAll(resolved)
	}

	return &fs.PathError{Op: "removeall", Path: name, Err: fs.ErrInvalid}
}

// Rename implements FullFileSystem
func (pfs *PathAwareFileSystem) Rename(oldpath, newpath string) error {
	resolvedOld, err := pfs.resolvePath(oldpath)
	if err != nil {
		return &fs.PathError{Op: "rename", Path: oldpath, Err: err}
	}

	resolvedNew, err := pfs.resolvePath(newpath)
	if err != nil {
		return &fs.PathError{Op: "rename", Path: newpath, Err: err}
	}

	if fullFS, ok := pfs.fs.(filesystem.FullFileSystem); ok {
		return fullFS.Rename(resolvedOld, resolvedNew)
	}

	return &fs.PathError{Op: "rename", Path: oldpath, Err: fs.ErrInvalid}
}

// Symlink implements FullFileSystem
func (pfs *PathAwareFileSystem) Symlink(oldname, newname string) error {
	// For symlinks, we need to be careful about the target
	// The target (oldname) might be relative to the link location
	resolvedNew, err := pfs.resolvePath(newname)
	if err != nil {
		return &fs.PathError{Op: "symlink", Path: newname, Err: err}
	}

	// The target can be absolute or relative
	// If it's relative, it's relative to the link's directory
	targetPath := oldname
	if filepath.IsAbs(oldname) {
		// Resolve absolute target
		targetPath, err = pfs.resolvePath(oldname)
		if err != nil {
			return &fs.PathError{Op: "symlink", Path: oldname, Err: err}
		}
	}

	if fullFS, ok := pfs.fs.(filesystem.FullFileSystem); ok {
		return fullFS.Symlink(targetPath, resolvedNew)
	}

	return &fs.PathError{Op: "symlink", Path: newname, Err: fs.ErrInvalid}
}

// Readlink implements FullFileSystem
func (pfs *PathAwareFileSystem) Readlink(name string) (string, error) {
	resolved, err := pfs.resolvePath(name)
	if err != nil {
		return "", &fs.PathError{Op: "readlink", Path: name, Err: err}
	}

	if fullFS, ok := pfs.fs.(filesystem.FullFileSystem); ok {
		return fullFS.Readlink(resolved)
	}

	return "", &fs.PathError{Op: "readlink", Path: name, Err: fs.ErrInvalid}
}

// resolvePath handles the path resolution, converting to relative for the underlying FS
func (pfs *PathAwareFileSystem) resolvePath(path string) (string, error) {
	// First resolve the path according to our rules
	resolved, err := pfs.handler.ResolvePath(path)
	if err != nil {
		return "", err
	}

	// Now make it relative for the underlying filesystem
	// The underlying FS expects relative paths from its root
	rel, err := pfs.handler.MakeRelative(resolved)
	if err != nil {
		// If we can't make it relative, try stripping the base
		if strings.HasPrefix(resolved, pfs.handler.base) {
			rel = strings.TrimPrefix(resolved, pfs.handler.base)
			rel = strings.TrimPrefix(rel, "/")
		} else {
			return "", err
		}
	}

	return rel, nil
}

// GetPathHandler returns the underlying path handler for direct access
func (pfs *PathAwareFileSystem) GetPathHandler() *PathHandler {
	return pfs.handler
}

// ResolveSymlinkTarget resolves a symlink target path to an absolute path within the filesystem root.
// This delegates to the path handler's security-critical symlink resolution function.
func (pfs *PathAwareFileSystem) ResolveSymlinkTarget(linkPath, targetPath string) (string, error) {
	return pfs.handler.ResolveSymlinkTarget(linkPath, targetPath)
}
