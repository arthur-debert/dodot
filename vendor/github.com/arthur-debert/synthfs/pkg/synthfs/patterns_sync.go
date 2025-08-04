package synthfs

import (
	"bytes"
	"context"
	"fmt"
	"io/fs"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// SyncOptions configures how directories are synchronized
type SyncOptions struct {
	// DeleteExtra removes files in destination that don't exist in source
	DeleteExtra bool
	// UpdateNewer only updates files if source is newer
	UpdateNewer bool
	// PreserveSymlinks copies symlinks as symlinks (not their targets)
	PreserveSymlinks bool
	// Filter determines which files to sync (return true to include)
	Filter func(path string, info fs.FileInfo) bool
	// DryRun reports what would be done without making changes
	DryRun bool
}

// SyncResult contains information about a sync operation
type SyncResult struct {
	FilesCreated    []string
	FilesUpdated    []string
	FilesDeleted    []string
	DirsCreated     []string
	DirsDeleted     []string
	SymlinksCreated []string
	Errors          []error
}

// SyncOperation synchronizes two directories
type SyncOperation struct {
	id      OperationID
	desc    OperationDesc
	srcDir  string
	dstDir  string
	options SyncOptions
	result  *SyncResult
}

// NewSyncOperation creates a new sync operation
func (s *SynthFS) NewSyncOperation(srcDir, dstDir string, opts ...SyncOptions) *SyncOperation {
	var options SyncOptions
	if len(opts) > 0 {
		options = opts[0]
	}

	// Default filter accepts everything
	if options.Filter == nil {
		options.Filter = func(path string, info fs.FileInfo) bool { return true }
	}

	id := s.idGen("sync", srcDir)
	return &SyncOperation{
		id: id,
		desc: OperationDesc{
			Type: "sync",
			Path: srcDir,
			Details: map[string]interface{}{
				"src":          srcDir,
				"dst":          dstDir,
				"delete_extra": options.DeleteExtra,
				"update_newer": options.UpdateNewer,
				"dry_run":      options.DryRun,
			},
		},
		srcDir:  srcDir,
		dstDir:  dstDir,
		options: options,
		result:  &SyncResult{},
	}
}

// GetResult returns the sync result after execution
func (op *SyncOperation) GetResult() *SyncResult {
	return op.result
}

// ID returns the operation ID
func (op *SyncOperation) ID() OperationID {
	return op.id
}

// Describe returns the operation description
func (op *SyncOperation) Describe() OperationDesc {
	return op.desc
}

// Dependencies returns empty - no dependencies
func (op *SyncOperation) Dependencies() []OperationID {
	return nil
}

// Conflicts returns empty - no conflicts
func (op *SyncOperation) Conflicts() []OperationID {
	return nil
}

// Prerequisites returns prerequisites for the operation
func (op *SyncOperation) Prerequisites() []core.Prerequisite {
	return []core.Prerequisite{
		core.NewSourceExistsPrerequisite(op.srcDir),
	}
}

// GetItem returns nil - no specific item
func (op *SyncOperation) GetItem() FsItem {
	return nil
}

// SetDescriptionDetail sets a detail in the description
func (op *SyncOperation) SetDescriptionDetail(key string, value interface{}) {
	if op.desc.Details == nil {
		op.desc.Details = make(map[string]interface{})
	}
	op.desc.Details[key] = value
}

// AddDependency adds a dependency
func (op *SyncOperation) AddDependency(depID OperationID) {
	// Not implemented for this operation
}

// SetPaths sets source and destination paths
func (op *SyncOperation) SetPaths(src, dst string) {
	op.srcDir = src
	op.dstDir = dst
	op.desc.Path = src
	op.desc.Details["src"] = src
	op.desc.Details["dst"] = dst
}

// GetChecksum returns nil
func (op *SyncOperation) GetChecksum(path string) *ChecksumRecord {
	return nil
}

// GetAllChecksums returns nil
func (op *SyncOperation) GetAllChecksums() map[string]*ChecksumRecord {
	return nil
}

// ExecuteV2 is not implemented
func (op *SyncOperation) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return fmt.Errorf("ExecuteV2 not implemented for SyncOperation")
}

// ValidateV2 is not implemented
func (op *SyncOperation) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return fmt.Errorf("ValidateV2 not implemented for SyncOperation")
}

// Rollback is not implemented yet
func (op *SyncOperation) Rollback(ctx context.Context, fsys FileSystem) error {
	return fmt.Errorf("rollback not implemented for SyncOperation")
}

// ReverseOps generates reverse operations
func (op *SyncOperation) ReverseOps(ctx context.Context, fsys FileSystem, budget *core.BackupBudget) ([]Operation, *core.BackupData, error) {
	return nil, nil, fmt.Errorf("reverse ops not implemented for SyncOperation")
}

// Execute performs the sync operation
func (op *SyncOperation) Execute(ctx context.Context, fsys FileSystem) error {
	// Reset result
	op.result = &SyncResult{}

	// Build source file map
	srcFiles := make(map[string]fs.FileInfo)
	err := op.walkDir(fsys, op.srcDir, "", func(relPath string, info fs.FileInfo) error {
		if op.options.Filter(relPath, info) {
			srcFiles[relPath] = info
		}
		return nil
	})
	if err != nil {
		return fmt.Errorf("failed to scan source: %w", err)
	}

	// Ensure destination exists before scanning
	if !op.options.DryRun {
		if writeFS, ok := fsys.(WriteFS); ok {
			if err := writeFS.MkdirAll(op.dstDir, 0755); err != nil {
				return fmt.Errorf("failed to create destination: %w", err)
			}
		}
	}

	// Build destination file map
	dstFiles := make(map[string]fs.FileInfo)
	err = op.walkDir(fsys, op.dstDir, "", func(relPath string, info fs.FileInfo) error {
		dstFiles[relPath] = info
		return nil
	})
	if err != nil && !isNotExist(err) {
		return fmt.Errorf("failed to scan destination: %w", err)
	}

	// Sync files from source to destination
	for relPath, srcInfo := range srcFiles {
		srcPath := filepath.Join(op.srcDir, relPath)
		dstPath := filepath.Join(op.dstDir, relPath)

		dstInfo, exists := dstFiles[relPath]

		if srcInfo.IsDir() {
			// Handle directory
			if !exists {
				if !op.options.DryRun {
					if writeFS, ok := fsys.(WriteFS); ok {
						if err := writeFS.MkdirAll(dstPath, srcInfo.Mode()); err != nil {
							op.result.Errors = append(op.result.Errors, err)
							continue
						}
					}
				}
				op.result.DirsCreated = append(op.result.DirsCreated, relPath)
			}
		} else if srcInfo.Mode()&fs.ModeSymlink != 0 && op.options.PreserveSymlinks {
			// Handle symlink
			if !exists {
				if !op.options.DryRun {
					if fullFS, ok := fsys.(FullFileSystem); ok {
						target, err := fullFS.Readlink(srcPath)
						if err == nil {
							if err := fullFS.Symlink(target, dstPath); err != nil {
								op.result.Errors = append(op.result.Errors, err)
								continue
							}
						}
					}
				}
				op.result.SymlinksCreated = append(op.result.SymlinksCreated, relPath)
			}
		} else {
			// Handle regular file
			shouldUpdate := false

			if !exists {
				shouldUpdate = true
				op.result.FilesCreated = append(op.result.FilesCreated, relPath)
			} else if !dstInfo.IsDir() {
				if op.options.UpdateNewer {
					// Only update if source is newer
					if srcInfo.ModTime().After(dstInfo.ModTime()) {
						shouldUpdate = true
						op.result.FilesUpdated = append(op.result.FilesUpdated, relPath)
					}
				} else {
					// Always update if content differs
					if !op.filesEqual(fsys, srcPath, dstPath) {
						shouldUpdate = true
						op.result.FilesUpdated = append(op.result.FilesUpdated, relPath)
					}
				}
			}

			if shouldUpdate && !op.options.DryRun {
				// Copy file
				content, err := fs.ReadFile(fsys, srcPath)
				if err != nil {
					op.result.Errors = append(op.result.Errors, err)
					continue
				}

				if writeFS, ok := fsys.(WriteFS); ok {
					// Ensure parent directory exists
					parent := filepath.Dir(dstPath)
					if parent != "." && parent != "/" {
						_ = writeFS.MkdirAll(parent, 0755)
					}

					if err := writeFS.WriteFile(dstPath, content, srcInfo.Mode()); err != nil {
						op.result.Errors = append(op.result.Errors, err)
					}
				}
			}
		}

		// Remove from destination map (for delete detection)
		delete(dstFiles, relPath)
	}

	// Handle extra files in destination
	if op.options.DeleteExtra {
		// Process directories first, then files
		// This ensures we count files in directories before deleting them
		var dirsToDelete []string
		deletedDirs := make(map[string]bool)

		// First pass: identify directories to delete
		for relPath, dstInfo := range dstFiles {
			if dstInfo.IsDir() {
				dirsToDelete = append(dirsToDelete, relPath)
				deletedDirs[relPath] = true
			}
		}

		// Second pass: count and delete files not in deleted directories
		for relPath, dstInfo := range dstFiles {
			if !dstInfo.IsDir() {
				// Check if this file is inside a directory we're deleting
				skipFile := false
				for dirPath := range deletedDirs {
					if strings.HasPrefix(relPath, dirPath+"/") {
						skipFile = true
						break
					}
				}

				if !skipFile {
					if !op.options.DryRun {
						if writeFS, ok := fsys.(WriteFS); ok {
							dstPath := filepath.Join(op.dstDir, relPath)
							if err := writeFS.Remove(dstPath); err != nil {
								op.result.Errors = append(op.result.Errors, err)
								continue
							}
						}
					}
					op.result.FilesDeleted = append(op.result.FilesDeleted, relPath)
				} else {
					// File is inside a directory being deleted, count it
					op.result.FilesDeleted = append(op.result.FilesDeleted, relPath)
				}
			}
		}

		// Third pass: delete directories
		for _, relPath := range dirsToDelete {
			if !op.options.DryRun {
				if writeFS, ok := fsys.(WriteFS); ok {
					dstPath := filepath.Join(op.dstDir, relPath)
					if err := writeFS.RemoveAll(dstPath); err != nil {
						op.result.Errors = append(op.result.Errors, err)
						continue
					}
				}
			}
			op.result.DirsDeleted = append(op.result.DirsDeleted, relPath)
		}
	}

	return nil
}

// walkDir walks a directory tree
func (op *SyncOperation) walkDir(fsys FileSystem, root, prefix string, fn func(string, fs.FileInfo) error) error {
	f, err := fsys.Open(root)
	if err != nil {
		return err
	}
	defer func() { _ = f.Close() }()

	if dirReader, ok := f.(interface {
		ReadDir(int) ([]fs.DirEntry, error)
	}); ok {
		entries, err := dirReader.ReadDir(-1)
		if err != nil {
			return err
		}

		for _, entry := range entries {
			name := entry.Name()
			path := filepath.Join(root, name)
			relPath := name
			if prefix != "" {
				relPath = filepath.Join(prefix, name)
			}

			info, err := entry.Info()
			if err != nil {
				continue
			}

			if err := fn(relPath, info); err != nil {
				return err
			}

			if entry.IsDir() {
				if err := op.walkDir(fsys, path, relPath, fn); err != nil {
					return err
				}
			}
		}
	}

	return nil
}

// filesEqual checks if two files have the same content
func (op *SyncOperation) filesEqual(fsys FileSystem, path1, path2 string) bool {
	content1, err1 := fs.ReadFile(fsys, path1)
	content2, err2 := fs.ReadFile(fsys, path2)

	if err1 != nil || err2 != nil {
		return false
	}

	return bytes.Equal(content1, content2)
}

// Validate checks if the operation can be performed
func (op *SyncOperation) Validate(ctx context.Context, fsys FileSystem) error {
	// Check if filesystem supports required operations
	if _, ok := fsys.(WriteFS); !ok {
		return fmt.Errorf("filesystem does not support write operations")
	}

	// Check if source exists
	if statFS, ok := fsys.(StatFS); ok {
		info, err := statFS.Stat(op.srcDir)
		if err != nil {
			return fmt.Errorf("source directory does not exist: %w", err)
		}
		if !info.IsDir() {
			return fmt.Errorf("source is not a directory: %s", op.srcDir)
		}
	}

	return nil
}

// Sync creates a sync operation
func (s *SynthFS) Sync(srcDir, dstDir string, opts ...SyncOptions) Operation {
	return s.NewSyncOperation(srcDir, dstDir, opts...)
}

// SyncDirectories is a convenience function that syncs directories directly
func SyncDirectories(ctx context.Context, fs FileSystem, srcDir, dstDir string, opts ...SyncOptions) (*SyncResult, error) {
	op := New().NewSyncOperation(srcDir, dstDir, opts...)
	err := op.Execute(ctx, fs)
	return op.GetResult(), err
}

// SyncBuilder provides a fluent interface for sync operations
type SyncBuilder struct {
	srcDir  string
	dstDir  string
	options SyncOptions
}

// NewSyncBuilder creates a new sync builder
func NewSyncBuilder(srcDir, dstDir string) *SyncBuilder {
	return &SyncBuilder{
		srcDir: srcDir,
		dstDir: dstDir,
		options: SyncOptions{
			Filter: func(path string, info fs.FileInfo) bool { return true },
		},
	}
}

// DeleteExtra enables deletion of extra files in destination
func (sb *SyncBuilder) DeleteExtra() *SyncBuilder {
	sb.options.DeleteExtra = true
	return sb
}

// UpdateNewer only updates files if source is newer
func (sb *SyncBuilder) UpdateNewer() *SyncBuilder {
	sb.options.UpdateNewer = true
	return sb
}

// PreserveSymlinks preserves symlinks as symlinks
func (sb *SyncBuilder) PreserveSymlinks() *SyncBuilder {
	sb.options.PreserveSymlinks = true
	return sb
}

// WithFilter sets a filter function
func (sb *SyncBuilder) WithFilter(filter func(path string, info fs.FileInfo) bool) *SyncBuilder {
	sb.options.Filter = filter
	return sb
}

// DryRun enables dry run mode
func (sb *SyncBuilder) DryRun() *SyncBuilder {
	sb.options.DryRun = true
	return sb
}

// Build creates the sync operation
func (sb *SyncBuilder) Build() Operation {
	return New().NewSyncOperation(sb.srcDir, sb.dstDir, sb.options)
}

// Execute builds and executes the operation
func (sb *SyncBuilder) Execute(ctx context.Context, fs FileSystem) (*SyncResult, error) {
	op := sb.Build().(*SyncOperation)
	err := op.Execute(ctx, fs)
	return op.GetResult(), err
}

// isNotExist checks if an error indicates a non-existent file
func isNotExist(err error) bool {
	if err == nil {
		return false
	}
	// Check for PathError with specific error
	if pathErr, ok := err.(*fs.PathError); ok {
		return pathErr.Err == fs.ErrNotExist
	}
	return err == fs.ErrNotExist
}
