package synthfs

import (
	"context"
	"fmt"
	"io/fs"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// MirrorOptions configures how a directory is mirrored with symlinks
type MirrorOptions struct {
	// Filter determines which files/dirs to include (return true to include)
	Filter func(path string, info fs.FileInfo) bool
	// IncludeDirectories creates directories instead of symlinking them
	IncludeDirectories bool
	// Overwrite existing symlinks
	Overwrite bool
}

// MirrorWithSymlinksOperation creates a directory structure with symlinks to original files
type MirrorWithSymlinksOperation struct {
	id      OperationID
	desc    OperationDesc
	srcDir  string
	dstDir  string
	options MirrorOptions
}

// NewMirrorWithSymlinksOperation creates a new mirror operation
func (s *SynthFS) NewMirrorWithSymlinksOperation(srcDir, dstDir string, opts ...MirrorOptions) *MirrorWithSymlinksOperation {
	var options MirrorOptions
	if len(opts) > 0 {
		options = opts[0]
	}

	// Default filter accepts everything
	if options.Filter == nil {
		options.Filter = func(path string, info fs.FileInfo) bool { return true }
	}

	id := s.idGen("mirror_symlinks", srcDir)
	return &MirrorWithSymlinksOperation{
		id: id,
		desc: OperationDesc{
			Type: "mirror_symlinks",
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
func (op *MirrorWithSymlinksOperation) ID() OperationID {
	return op.id
}

// Describe returns the operation description
func (op *MirrorWithSymlinksOperation) Describe() OperationDesc {
	return op.desc
}

// Dependencies returns empty - no dependencies
func (op *MirrorWithSymlinksOperation) Dependencies() []OperationID {
	return nil
}

// Conflicts returns empty - no conflicts
func (op *MirrorWithSymlinksOperation) Conflicts() []OperationID {
	return nil
}

// Prerequisites returns prerequisites for the operation
func (op *MirrorWithSymlinksOperation) Prerequisites() []core.Prerequisite {
	return []core.Prerequisite{
		core.NewSourceExistsPrerequisite(op.srcDir),
		core.NewParentDirPrerequisite(op.dstDir),
	}
}

// GetItem returns nil - no specific item
func (op *MirrorWithSymlinksOperation) GetItem() FsItem {
	return nil
}

// SetDescriptionDetail sets a detail in the description
func (op *MirrorWithSymlinksOperation) SetDescriptionDetail(key string, value interface{}) {
	if op.desc.Details == nil {
		op.desc.Details = make(map[string]interface{})
	}
	op.desc.Details[key] = value
}

// AddDependency adds a dependency
func (op *MirrorWithSymlinksOperation) AddDependency(depID OperationID) {
	// Not implemented for this operation
}

// SetPaths sets source and destination paths
func (op *MirrorWithSymlinksOperation) SetPaths(src, dst string) {
	op.srcDir = src
	op.dstDir = dst
	op.desc.Path = src
	op.desc.Details["src"] = src
	op.desc.Details["dst"] = dst
}

// GetChecksum returns nil
func (op *MirrorWithSymlinksOperation) GetChecksum(path string) *ChecksumRecord {
	return nil
}

// GetAllChecksums returns nil
func (op *MirrorWithSymlinksOperation) GetAllChecksums() map[string]*ChecksumRecord {
	return nil
}

// ExecuteV2 is not implemented
func (op *MirrorWithSymlinksOperation) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return fmt.Errorf("ExecuteV2 not implemented for MirrorWithSymlinksOperation")
}

// ValidateV2 is not implemented
func (op *MirrorWithSymlinksOperation) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return fmt.Errorf("ValidateV2 not implemented for MirrorWithSymlinksOperation")
}

// Rollback is not implemented yet
func (op *MirrorWithSymlinksOperation) Rollback(ctx context.Context, fsys FileSystem) error {
	return fmt.Errorf("rollback not implemented for MirrorWithSymlinksOperation")
}

// ReverseOps generates reverse operations
func (op *MirrorWithSymlinksOperation) ReverseOps(ctx context.Context, fsys FileSystem, budget *core.BackupBudget) ([]Operation, *core.BackupData, error) {
	return nil, nil, fmt.Errorf("reverse ops not implemented for MirrorWithSymlinksOperation")
}

// Execute performs the mirror operation
func (op *MirrorWithSymlinksOperation) Execute(ctx context.Context, fsys FileSystem) error {
	// Check if filesystem supports full operations
	fullFS, ok := fsys.(FullFileSystem)
	if !ok {
		return fmt.Errorf("filesystem does not support full operations (symlinks)")
	}

	// Recursive walk function
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

				// Calculate relative and destination paths
				relPath, err := filepath.Rel(op.srcDir, srcPath)
				if err != nil {
					continue
				}

				dstPath := filepath.Join(op.dstDir, relPath)

				if entry.IsDir() {
					if op.options.IncludeDirectories {
						// Create real directory
						err = fullFS.MkdirAll(dstPath, info.Mode())
						if err != nil && !strings.Contains(err.Error(), "exists") {
							return err
						}

						// Recurse into directory
						if err := walk(srcPath); err != nil {
							return err
						}
					} else {
						// Create parent directory if needed
						dstParent := filepath.Dir(dstPath)
						if dstParent != "." && dstParent != "/" {
							err = fullFS.MkdirAll(dstParent, 0755)
							if err != nil && !strings.Contains(err.Error(), "exists") {
								return err
							}
						}

						// Calculate relative path from destination to source
						relTarget, _ := filepath.Rel(filepath.Dir(dstPath), srcPath)

						// Use PathAwareFileSystem if available for secure symlink resolution
						var resolvedTarget string
						if pafs, ok := fsys.(*PathAwareFileSystem); ok {
							// Use centralized security-aware symlink resolution
							resolved, err := pafs.ResolveSymlinkTarget(dstPath, relTarget)
							if err != nil {
								return fmt.Errorf("failed to resolve symlink target for %s -> %s: %w", dstPath, relTarget, err)
							}
							resolvedTarget = resolved
						} else {
							// Fallback for non-PathAwareFileSystem (should not happen in practice)
							resolvedTarget = srcPath
						}

						// Remove existing if overwrite is enabled
						if op.options.Overwrite {
							_ = fullFS.Remove(dstPath)
						}

						err = fullFS.Symlink(resolvedTarget, dstPath)
						if err != nil && !strings.Contains(err.Error(), "exists") {
							return fmt.Errorf("failed to create symlink %s -> %s: %w", dstPath, resolvedTarget, err)
						}
					}
				} else {
					// Create parent directory if needed
					dstParent := filepath.Dir(dstPath)
					if dstParent != "." && dstParent != "/" {
						err = fullFS.MkdirAll(dstParent, 0755)
						if err != nil && !strings.Contains(err.Error(), "exists") {
							return err
						}
					}

					// Calculate relative path from destination to source
					relTarget, _ := filepath.Rel(filepath.Dir(dstPath), srcPath)

					// Use PathAwareFileSystem if available for secure symlink resolution
					var resolvedTarget string
					if pafs, ok := fsys.(*PathAwareFileSystem); ok {
						// Use centralized security-aware symlink resolution
						resolved, err := pafs.ResolveSymlinkTarget(dstPath, relTarget)
						if err != nil {
							return fmt.Errorf("failed to resolve symlink target for %s -> %s: %w", dstPath, relTarget, err)
						}
						resolvedTarget = resolved
					} else {
						// Fallback for non-PathAwareFileSystem (should not happen in practice)
						resolvedTarget = srcPath
					}

					// Remove existing if overwrite is enabled
					if op.options.Overwrite {
						_ = fullFS.Remove(dstPath)
					}

					err = fullFS.Symlink(resolvedTarget, dstPath)
					if err != nil && !strings.Contains(err.Error(), "exists") {
						return fmt.Errorf("failed to create symlink %s -> %s: %w", dstPath, resolvedTarget, err)
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
func (op *MirrorWithSymlinksOperation) Validate(ctx context.Context, fsys FileSystem) error {
	// Check if filesystem supports Stat
	statFS, ok := fsys.(StatFS)
	if !ok {
		return fmt.Errorf("filesystem does not support Stat")
	}

	// Check if filesystem supports symlinks
	if _, ok := fsys.(FullFileSystem); !ok {
		return fmt.Errorf("filesystem does not support symlinks")
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

// MirrorBuilder provides a fluent interface for creating mirror operations
type MirrorBuilder struct {
	srcDir  string
	dstDir  string
	options MirrorOptions
}

// NewMirrorBuilder creates a new mirror builder
func NewMirrorBuilder(srcDir, dstDir string) *MirrorBuilder {
	return &MirrorBuilder{
		srcDir: srcDir,
		dstDir: dstDir,
		options: MirrorOptions{
			Filter: func(path string, info fs.FileInfo) bool { return true },
		},
	}
}

// WithFilter sets a filter function
func (b *MirrorBuilder) WithFilter(filter func(path string, info fs.FileInfo) bool) *MirrorBuilder {
	b.options.Filter = filter
	return b
}

// ExcludeHidden excludes hidden files (starting with .)
func (b *MirrorBuilder) ExcludeHidden() *MirrorBuilder {
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

// IncludeDirectories creates real directories instead of symlinking them
func (b *MirrorBuilder) IncludeDirectories() *MirrorBuilder {
	b.options.IncludeDirectories = true
	return b
}

// Overwrite enables overwriting existing symlinks
func (b *MirrorBuilder) Overwrite() *MirrorBuilder {
	b.options.Overwrite = true
	return b
}

// Build creates the mirror operation
func (b *MirrorBuilder) Build() Operation {
	return New().NewMirrorWithSymlinksOperation(b.srcDir, b.dstDir, b.options)
}

// Execute builds and executes the operation
func (b *MirrorBuilder) Execute(ctx context.Context, fs FileSystem) error {
	op := b.Build()
	return op.Execute(ctx, fs)
}

// MirrorWithSymlinks is a convenience function
func MirrorWithSymlinks(ctx context.Context, fs FileSystem, srcDir, dstDir string) error {
	op := New().NewMirrorWithSymlinksOperation(srcDir, dstDir)
	return op.Execute(ctx, fs)
}
