package synthfs

import (
	"context"
	"fmt"
	"io/fs"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// CopyTreeOptions configures how a directory tree is copied
type CopyTreeOptions struct {
	// Filter determines which files/dirs to include (return true to include)
	Filter func(path string, info fs.FileInfo) bool
	// PreservePermissions copies file permissions
	PreservePermissions bool
	// PreserveTimestamps copies modification times
	PreserveTimestamps bool
	// FollowSymlinks follows symlinks instead of copying them
	FollowSymlinks bool
	// Overwrite existing files
	Overwrite bool
}

// CopyTree creates operations to recursively copy a directory tree
func (s *SynthFS) CopyTree(srcDir, dstDir string, opts ...CopyTreeOptions) ([]Operation, error) {
	var options CopyTreeOptions
	if len(opts) > 0 {
		options = opts[0]
	}

	// Default filter accepts everything
	if options.Filter == nil {
		options.Filter = func(path string, info fs.FileInfo) bool { return true }
	}

	var ops []Operation

	// Walk the source directory and create copy operations
	_ = func(srcPath string, dstPath string, info fs.FileInfo) error {
		// Apply filter
		if !options.Filter(srcPath, info) {
			return nil
		}

		relPath, err := filepath.Rel(srcDir, srcPath)
		if err != nil {
			return err
		}

		targetPath := filepath.Join(dstDir, relPath)

		if info.IsDir() {
			// Create directory
			op := s.CreateDir(targetPath, info.Mode())
			ops = append(ops, op)
		} else if info.Mode()&fs.ModeSymlink != 0 && !options.FollowSymlinks {
			// Copy symlink
			op := s.CreateSymlink("", targetPath) // We'd need to read the link target
			ops = append(ops, op)
		} else {
			// Copy file
			op := s.Copy(srcPath, targetPath)
			if options.PreservePermissions {
				if adapter, ok := op.(*OperationsPackageAdapter); ok {
					adapter.SetDescriptionDetail("preserve_mode", true)
					adapter.SetDescriptionDetail("mode", info.Mode())
				}
			}
			ops = append(ops, op)
		}

		return nil
	}

	// Since we can't actually walk the filesystem here (no fs parameter),
	// we'll return a function that creates the operations when executed
	// For now, we'll create a custom operation that handles this
	return nil, fmt.Errorf("CopyTree requires filesystem access - use CopyTreeOperation instead")
}

// CopyTreeOperation is a special operation that copies an entire directory tree
type CopyTreeOperation struct {
	id      OperationID
	desc    OperationDesc
	srcDir  string
	dstDir  string
	options CopyTreeOptions
}

// NewCopyTreeOperation creates a new copy tree operation
func (s *SynthFS) NewCopyTreeOperation(srcDir, dstDir string, opts ...CopyTreeOptions) *CopyTreeOperation {
	var options CopyTreeOptions
	if len(opts) > 0 {
		options = opts[0]
	}

	// Default filter accepts everything
	if options.Filter == nil {
		options.Filter = func(path string, info fs.FileInfo) bool { return true }
	}

	id := s.idGen("copy_tree", srcDir)
	return &CopyTreeOperation{
		id: id,
		desc: OperationDesc{
			Type: "copy_tree",
			Path: srcDir,
			Details: map[string]interface{}{
				"src": srcDir,
				"dst": dstDir,
			},
		},
		srcDir:  srcDir,
		dstDir:  dstDir,
		options: options,
	}
}

// ID returns the operation ID
func (op *CopyTreeOperation) ID() OperationID {
	return op.id
}

// Describe returns the operation description
func (op *CopyTreeOperation) Describe() OperationDesc {
	return op.desc
}

// Dependencies returns empty - no dependencies
func (op *CopyTreeOperation) Dependencies() []OperationID {
	return nil
}

// Conflicts returns empty - no conflicts
func (op *CopyTreeOperation) Conflicts() []OperationID {
	return nil
}

// Prerequisites returns prerequisites for the operation
func (op *CopyTreeOperation) Prerequisites() []core.Prerequisite {
	return []core.Prerequisite{
		core.NewSourceExistsPrerequisite(op.srcDir),
		core.NewParentDirPrerequisite(op.dstDir),
	}
}

// GetItem returns nil - no specific item
func (op *CopyTreeOperation) GetItem() FsItem {
	return nil
}

// SetDescriptionDetail sets a detail in the description
func (op *CopyTreeOperation) SetDescriptionDetail(key string, value interface{}) {
	if op.desc.Details == nil {
		op.desc.Details = make(map[string]interface{})
	}
	op.desc.Details[key] = value
}

// AddDependency adds a dependency
func (op *CopyTreeOperation) AddDependency(depID OperationID) {
	// Not implemented for this operation
}

// SetPaths sets source and destination paths
func (op *CopyTreeOperation) SetPaths(src, dst string) {
	op.srcDir = src
	op.dstDir = dst
	op.desc.Path = src
	op.desc.Details["src"] = src
	op.desc.Details["dst"] = dst
}

// GetChecksum returns nil
func (op *CopyTreeOperation) GetChecksum(path string) *ChecksumRecord {
	return nil
}

// GetAllChecksums returns nil
func (op *CopyTreeOperation) GetAllChecksums() map[string]*ChecksumRecord {
	return nil
}

// ExecuteV2 is not implemented
func (op *CopyTreeOperation) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return fmt.Errorf("ExecuteV2 not implemented for CopyTreeOperation")
}

// ValidateV2 is not implemented
func (op *CopyTreeOperation) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return fmt.Errorf("ValidateV2 not implemented for CopyTreeOperation")
}

// Rollback is not implemented yet
func (op *CopyTreeOperation) Rollback(ctx context.Context, fsys FileSystem) error {
	// Would need to track all created files/dirs
	return fmt.Errorf("rollback not implemented for CopyTreeOperation")
}

// ReverseOps generates reverse operations
func (op *CopyTreeOperation) ReverseOps(ctx context.Context, fsys FileSystem, budget *core.BackupBudget) ([]Operation, *core.BackupData, error) {
	// Would create a delete operation for the destination
	return nil, nil, fmt.Errorf("reverse ops not implemented for CopyTreeOperation")
}

// Execute performs the copy tree operation
func (op *CopyTreeOperation) Execute(ctx context.Context, fsys FileSystem) error {
	// Check if filesystem supports full operations
	fullFS, ok := fsys.(FullFileSystem)
	if !ok {
		return fmt.Errorf("filesystem does not support full operations")
	}

	// We need a way to walk the filesystem
	// For now, we'll use a simple recursive approach
	var walk func(string) error
	walk = func(dir string) error {
		// Open directory
		f, err := fsys.Open(dir)
		if err != nil {
			return err
		}
		defer func() { _ = f.Close() }()

		// Read directory entries
		if dirReader, ok := f.(interface {
			ReadDir(int) ([]fs.DirEntry, error)
		}); ok {
			entries, err := dirReader.ReadDir(-1)
			if err != nil {
				return err
			}

			for _, entry := range entries {
				srcPath := filepath.Join(dir, entry.Name())
				info, err := entry.Info()
				if err != nil {
					continue
				}

				// Apply filter
				if !op.options.Filter(srcPath, info) {
					continue
				}

				relPath, err := filepath.Rel(op.srcDir, srcPath)
				if err != nil {
					continue
				}

				dstPath := filepath.Join(op.dstDir, relPath)

				if entry.IsDir() {
					// Create directory
					err = fullFS.MkdirAll(dstPath, info.Mode())
					if err != nil && !strings.Contains(err.Error(), "exists") {
						return err
					}

					// Recurse
					if err := walk(srcPath); err != nil {
						return err
					}
				} else if info.Mode()&fs.ModeSymlink != 0 && !op.options.FollowSymlinks {
					// Read symlink
					target, err := fullFS.Readlink(srcPath)
					if err != nil {
						continue
					}

					// Create symlink
					_ = fullFS.Symlink(target, dstPath)
				} else {
					// Copy file
					content, err := fs.ReadFile(fsys, srcPath)
					if err != nil {
						continue
					}

					mode := fs.FileMode(0644)
					if op.options.PreservePermissions {
						mode = info.Mode()
					}

					err = fullFS.WriteFile(dstPath, content, mode)
					if err != nil && !op.options.Overwrite {
						return err
					}
				}
			}
		}

		return nil
	}

	// Create destination directory
	if err := fullFS.MkdirAll(op.dstDir, 0755); err != nil {
		return err
	}

	// Start walking from source
	return walk(op.srcDir)
}

// Validate checks if the operation can be performed
func (op *CopyTreeOperation) Validate(ctx context.Context, fsys FileSystem) error {
	// Check if filesystem supports Stat
	statFS, ok := fsys.(StatFS)
	if !ok {
		return fmt.Errorf("filesystem does not support Stat")
	}

	// Check source exists
	info, err := statFS.Stat(op.srcDir)
	if err != nil {
		return fmt.Errorf("source directory does not exist: %w", err)
	}

	if !info.IsDir() {
		return fmt.Errorf("source is not a directory: %s", op.srcDir)
	}

	// Check if destination exists
	if _, err := statFS.Stat(op.dstDir); err == nil && !op.options.Overwrite {
		return fmt.Errorf("destination already exists: %s", op.dstDir)
	}

	return nil
}

// CopyTreeBuilder provides a fluent interface for configuring tree copies
type CopyTreeBuilder struct {
	srcDir  string
	dstDir  string
	options CopyTreeOptions
}

// NewCopyTreeBuilder creates a new copy tree builder
func NewCopyTreeBuilder(srcDir, dstDir string) *CopyTreeBuilder {
	return &CopyTreeBuilder{
		srcDir: srcDir,
		dstDir: dstDir,
		options: CopyTreeOptions{
			Filter: func(path string, info fs.FileInfo) bool { return true },
		},
	}
}

// WithFilter sets a filter function
func (b *CopyTreeBuilder) WithFilter(filter func(path string, info fs.FileInfo) bool) *CopyTreeBuilder {
	b.options.Filter = filter
	return b
}

// ExcludeHidden excludes hidden files (starting with .)
func (b *CopyTreeBuilder) ExcludeHidden() *CopyTreeBuilder {
	originalFilter := b.options.Filter
	b.options.Filter = func(path string, info fs.FileInfo) bool {
		name := filepath.Base(path)
		if strings.HasPrefix(name, ".") {
			return false
		}
		return originalFilter(path, info)
	}
	return b
}

// ExcludePattern excludes files matching the pattern
func (b *CopyTreeBuilder) ExcludePattern(pattern string) *CopyTreeBuilder {
	originalFilter := b.options.Filter
	b.options.Filter = func(path string, info fs.FileInfo) bool {
		matched, _ := filepath.Match(pattern, filepath.Base(path))
		if matched {
			return false
		}
		return originalFilter(path, info)
	}
	return b
}

// PreservePermissions enables permission preservation
func (b *CopyTreeBuilder) PreservePermissions() *CopyTreeBuilder {
	b.options.PreservePermissions = true
	return b
}

// PreserveTimestamps enables timestamp preservation
func (b *CopyTreeBuilder) PreserveTimestamps() *CopyTreeBuilder {
	b.options.PreserveTimestamps = true
	return b
}

// FollowSymlinks enables following symlinks
func (b *CopyTreeBuilder) FollowSymlinks() *CopyTreeBuilder {
	b.options.FollowSymlinks = true
	return b
}

// Overwrite enables overwriting existing files
func (b *CopyTreeBuilder) Overwrite() *CopyTreeBuilder {
	b.options.Overwrite = true
	return b
}

// Build creates the copy tree operation
func (b *CopyTreeBuilder) Build() Operation {
	return New().NewCopyTreeOperation(b.srcDir, b.dstDir, b.options)
}

// Execute builds and executes the operation
func (b *CopyTreeBuilder) Execute(ctx context.Context, fs FileSystem) error {
	op := b.Build()
	return op.Execute(ctx, fs)
}

// CopyTreeFunc is a convenience function for copying directory trees
func CopyTreeFunc(ctx context.Context, fs FileSystem, srcDir, dstDir string) error {
	op := New().NewCopyTreeOperation(srcDir, dstDir)
	return op.Execute(ctx, fs)
}
