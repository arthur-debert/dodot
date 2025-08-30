package filesystem

import (
	"io/fs"
	"os"

	"github.com/arthur-debert/dodot/pkg/types"
)

// osFS implements types.FS using the OS filesystem
type osFS struct{}

// NewOS creates a new OS filesystem implementation
func NewOS() types.FS {
	return &osFS{}
}

func (o *osFS) Stat(name string) (fs.FileInfo, error) {
	return os.Stat(name)
}

func (o *osFS) ReadFile(name string) ([]byte, error) {
	return os.ReadFile(name)
}

func (o *osFS) WriteFile(name string, data []byte, perm fs.FileMode) error {
	return os.WriteFile(name, data, perm)
}

func (o *osFS) MkdirAll(path string, perm fs.FileMode) error {
	return os.MkdirAll(path, perm)
}

func (o *osFS) Symlink(oldname, newname string) error {
	return os.Symlink(oldname, newname)
}

func (o *osFS) Readlink(name string) (string, error) {
	return os.Readlink(name)
}

func (o *osFS) Remove(name string) error {
	return os.Remove(name)
}

func (o *osFS) RemoveAll(path string) error {
	return os.RemoveAll(path)
}

func (o *osFS) Rename(oldpath, newpath string) error {
	return os.Rename(oldpath, newpath)
}

func (o *osFS) Lstat(name string) (fs.FileInfo, error) {
	return os.Lstat(name)
}

func (o *osFS) ReadDir(name string) ([]fs.DirEntry, error) {
	return os.ReadDir(name)
}
