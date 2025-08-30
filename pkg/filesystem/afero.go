package filesystem

import (
	"io/fs"
	"os"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/spf13/afero"
)

// aferoFS implements types.FS using afero
type aferoFS struct {
	fs afero.Fs
}

// NewAferoFS creates a new afero filesystem implementation
func NewAferoFS(fs afero.Fs) types.FS {
	return &aferoFS{fs: fs}
}

func (a *aferoFS) Stat(name string) (fs.FileInfo, error) {
	return a.fs.Stat(name)
}

func (a *aferoFS) ReadFile(name string) ([]byte, error) {
	info, err := a.fs.Stat(name)
	if err != nil {
		return nil, err
	}
	if info.IsDir() {
		return nil, &fs.PathError{Op: "read", Path: name, Err: fs.ErrInvalid}
	}
	return afero.ReadFile(a.fs, name)
}

func (a *aferoFS) WriteFile(name string, data []byte, perm fs.FileMode) error {
	return afero.WriteFile(a.fs, name, data, perm)
}

func (a *aferoFS) MkdirAll(path string, perm fs.FileMode) error {
	return a.fs.MkdirAll(path, perm)
}

func (a *aferoFS) Symlink(oldname, newname string) error {
	// Afero's MemMapFs doesn't support Symlink, so we simulate it
	// by creating a file with the symlink target as content.
	// This is a limitation of afero, but sufficient for many tests.
	return afero.WriteFile(a.fs, newname, []byte(oldname), 0777|os.ModeSymlink)
}

func (a *aferoFS) Readlink(name string) (string, error) {
	// Fallback for filesystems that don't support symlinks
	content, err := afero.ReadFile(a.fs, name)
	if err != nil {
		return "", err
	}
	return string(content), nil
}

func (a *aferoFS) Remove(name string) error {
	return a.fs.Remove(name)
}

func (a *aferoFS) RemoveAll(path string) error {
	return a.fs.RemoveAll(path)
}

func (a *aferoFS) Rename(oldpath, newpath string) error {
	return a.fs.Rename(oldpath, newpath)
}

func (a *aferoFS) Lstat(name string) (fs.FileInfo, error) {
	// Afero's Lstat is only available on the OsFs.
	// For MemMapFs, Stat is sufficient for most tests.
	return a.fs.Stat(name)
}

func (a *aferoFS) ReadDir(name string) ([]fs.DirEntry, error) {
	entries, err := afero.ReadDir(a.fs, name)
	if err != nil {
		return nil, err
	}
	dirEntries := make([]fs.DirEntry, len(entries))
	for i, entry := range entries {
		dirEntries[i] = fs.FileInfoToDirEntry(entry)
	}
	return dirEntries, nil
}
