package filesystem

import (
	"io/fs"
	"os"
	"path/filepath"
	"strings"
)

// OSFileSystem implements FullFileSystem using the OS filesystem
type OSFileSystem struct {
	root string
}

// NewOSFileSystem creates a new OS-based filesystem rooted at the given path
func NewOSFileSystem(root string) *OSFileSystem {
	return &OSFileSystem{root: root}
}

// Open implements fs.FS
func (osfs *OSFileSystem) Open(name string) (fs.File, error) {
	if !fs.ValidPath(name) {
		return nil, &fs.PathError{Op: "open", Path: name, Err: fs.ErrInvalid}
	}
	fullPath := filepath.Join(osfs.root, name)
	return os.Open(fullPath)
}

// Stat implements StatFS
func (osfs *OSFileSystem) Stat(name string) (fs.FileInfo, error) {
	if !fs.ValidPath(name) {
		return nil, &fs.PathError{Op: "stat", Path: name, Err: fs.ErrInvalid}
	}
	fullPath := filepath.Join(osfs.root, name)
	return os.Stat(fullPath)
}

// WriteFile implements WriteFS
func (osfs *OSFileSystem) WriteFile(name string, data []byte, perm fs.FileMode) error {
	if !fs.ValidPath(name) {
		return &fs.PathError{Op: "writefile", Path: name, Err: fs.ErrInvalid}
	}
	fullPath := filepath.Join(osfs.root, name)
	return os.WriteFile(fullPath, data, perm)
}

// MkdirAll implements WriteFS
func (osfs *OSFileSystem) MkdirAll(path string, perm fs.FileMode) error {
	if !fs.ValidPath(path) {
		return &fs.PathError{Op: "mkdirall", Path: path, Err: fs.ErrInvalid}
	}
	fullPath := filepath.Join(osfs.root, path)
	return os.MkdirAll(fullPath, perm)
}

// Remove implements WriteFS
func (osfs *OSFileSystem) Remove(name string) error {
	if !fs.ValidPath(name) {
		return &fs.PathError{Op: "remove", Path: name, Err: fs.ErrInvalid}
	}
	fullPath := filepath.Join(osfs.root, name)
	return os.Remove(fullPath)
}

// RemoveAll implements WriteFS
func (osfs *OSFileSystem) RemoveAll(name string) error {
	if !fs.ValidPath(name) {
		return &fs.PathError{Op: "removeall", Path: name, Err: fs.ErrInvalid}
	}
	fullPath := filepath.Join(osfs.root, name)
	return os.RemoveAll(fullPath)
}

// Symlink implements WriteFS
func (osfs *OSFileSystem) Symlink(oldname, newname string) error {
	if !fs.ValidPath(oldname) || !fs.ValidPath(newname) {
		return &fs.PathError{Op: "symlink", Path: newname, Err: fs.ErrInvalid}
	}
	oldPath := filepath.Join(osfs.root, oldname)
	newPath := filepath.Join(osfs.root, newname)
	return os.Symlink(oldPath, newPath)
}

// Readlink implements WriteFS
func (osfs *OSFileSystem) Readlink(name string) (string, error) {
	if !fs.ValidPath(name) {
		return "", &fs.PathError{Op: "readlink", Path: name, Err: fs.ErrInvalid}
	}
	fullPath := filepath.Join(osfs.root, name)
	target, err := os.Readlink(fullPath)
	if err != nil {
		return "", err
	}
	// Convert absolute path back to relative if it's within our root
	if filepath.IsAbs(target) {
		rel, err := filepath.Rel(osfs.root, target)
		if err == nil && !strings.HasPrefix(rel, "..") {
			return rel, nil
		}
	}
	return target, nil
}

// Rename implements WriteFS
func (osfs *OSFileSystem) Rename(oldpath, newpath string) error {
	if !fs.ValidPath(oldpath) || !fs.ValidPath(newpath) {
		return &fs.PathError{Op: "rename", Path: newpath, Err: fs.ErrInvalid}
	}
	oldFullPath := filepath.Join(osfs.root, oldpath)
	newFullPath := filepath.Join(osfs.root, newpath)
	return os.Rename(oldFullPath, newFullPath)
}
