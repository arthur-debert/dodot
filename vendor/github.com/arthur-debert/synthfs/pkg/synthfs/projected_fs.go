package synthfs

import (
	"io/fs"
	"path/filepath"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// projectedFileInfo implements fs.FileInfo for projected files
type projectedFileInfo struct {
	name      string
	size      int64
	mode      fs.FileMode
	modTime   time.Time
	isDir     bool
	isSymlink bool
}

func (pfi *projectedFileInfo) Name() string       { return pfi.name }
func (pfi *projectedFileInfo) Size() int64        { return pfi.size }
func (pfi *projectedFileInfo) Mode() fs.FileMode  { return pfi.mode }
func (pfi *projectedFileInfo) ModTime() time.Time { return pfi.modTime }
func (pfi *projectedFileInfo) IsDir() bool        { return pfi.isDir }
func (pfi *projectedFileInfo) Sys() interface{}   { return nil }

// ProjectedFileSystem wraps a real filesystem and overlays projected state from operations
type ProjectedFileSystem struct {
	realFS  filesystem.FullFileSystem
	tracker *PathStateTracker
}

// NewProjectedFileSystem creates a new projected filesystem
func NewProjectedFileSystem(fs filesystem.FullFileSystem) *ProjectedFileSystem {
	return &ProjectedFileSystem{
		realFS:  fs,
		tracker: NewPathStateTracker(fs),
	}
}

// UpdateProjectedState updates the projected state based on an operation
func (pfs *ProjectedFileSystem) UpdateProjectedState(op Operation) error {
	return pfs.tracker.UpdateState(op)
}

// Stat returns file info, checking projected state first
func (pfs *ProjectedFileSystem) Stat(path string) (fs.FileInfo, error) {
	// Check projected state first
	state, err := pfs.tracker.GetState(path)
	if err == nil && state != nil {
		// If the path will be deleted, return not exist
		if state.DeletedBy != "" {
			return nil, &fs.PathError{Op: "stat", Path: path, Err: fs.ErrNotExist}
		}

		// If the path will exist, return projected info
		if state.WillExist {
			mode := fs.FileMode(0644)
			isDir := false
			isSymlink := false

			switch state.WillBeType {
			case PathStateDir:
				mode = fs.FileMode(0755) | fs.ModeDir
				isDir = true
			case PathStateSymlink:
				mode = fs.FileMode(0777) | fs.ModeSymlink
				isSymlink = true
			}

			return &projectedFileInfo{
				name:      filepath.Base(path),
				size:      0, // We don't track size in projected state
				mode:      mode,
				modTime:   time.Now(),
				isDir:     isDir,
				isSymlink: isSymlink,
			}, nil
		}
	}

	// Fall back to real filesystem
	return pfs.realFS.Stat(path)
}

// Lstat returns file info without following symlinks, checking projected state first
func (pfs *ProjectedFileSystem) Lstat(path string) (fs.FileInfo, error) {
	// For projected state, Lstat behaves the same as Stat
	// since we don't actually follow symlinks in projected state
	return pfs.Stat(path)
}

// The following methods simply delegate to the real filesystem
// since they don't need projected state handling

func (pfs *ProjectedFileSystem) Open(name string) (fs.File, error) {
	return pfs.realFS.Open(name)
}

func (pfs *ProjectedFileSystem) MkdirAll(path string, perm fs.FileMode) error {
	return pfs.realFS.MkdirAll(path, perm)
}

func (pfs *ProjectedFileSystem) Remove(name string) error {
	return pfs.realFS.Remove(name)
}

func (pfs *ProjectedFileSystem) RemoveAll(path string) error {
	return pfs.realFS.RemoveAll(path)
}

func (pfs *ProjectedFileSystem) Rename(oldpath, newpath string) error {
	return pfs.realFS.Rename(oldpath, newpath)
}

func (pfs *ProjectedFileSystem) Symlink(oldname, newname string) error {
	return pfs.realFS.Symlink(oldname, newname)
}

func (pfs *ProjectedFileSystem) Readlink(name string) (string, error) {
	return pfs.realFS.Readlink(name)
}

func (pfs *ProjectedFileSystem) WriteFile(name string, data []byte, perm fs.FileMode) error {
	return pfs.realFS.WriteFile(name, data, perm)
}

// Ensure ProjectedFileSystem implements FullFileSystem
var _ filesystem.FullFileSystem = (*ProjectedFileSystem)(nil)
