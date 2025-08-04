package synthfs

import (
	"context"
	"fmt"
	"io/fs"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// StructureEntry represents a single entry in a directory structure
type StructureEntry struct {
	Path      string
	IsDir     bool
	Content   []byte
	Mode      fs.FileMode
	IsSymlink bool
	Target    string
}

// ParseStructure parses a tree-like structure definition
func ParseStructure(structure string) ([]StructureEntry, error) {
	var entries []StructureEntry
	lines := strings.Split(structure, "\n")

	// Track directory stack for path construction
	dirStack := []string{}
	lastDepth := -1

	for _, line := range lines {
		if strings.TrimSpace(line) == "" {
			continue
		}

		// Count leading spaces/tabs to determine depth
		// For tree-style format, we need to handle tree characters specially
		depth := 0
		hasTreeChars := strings.ContainsAny(line, "│├└")

		if hasTreeChars {
			// For tree format, count the │ prefixes first
			tempLine := line
			for strings.HasPrefix(tempLine, "│   ") {
				depth++
				tempLine = tempLine[4:]
			}
			// Trim any spaces between │ and branch characters
			tempLine = strings.TrimLeft(tempLine, " ")
			// Then check for branch characters
			if strings.HasPrefix(tempLine, "├── ") || strings.HasPrefix(tempLine, "└── ") {
				depth++
			}
		} else {
			// Normal indentation counting
			for i, ch := range line {
				if ch == ' ' || ch == '\t' {
					depth = i + 1
				} else {
					break
				}
			}
			// Normalize depth (4 spaces = 1 level)
			depth = depth / 4
		}

		// Adjust directory stack based on depth
		if depth <= lastDepth {
			// Pop directories to match depth
			keep := depth
			if keep < 0 {
				keep = 0
			}
			if keep < len(dirStack) {
				dirStack = dirStack[:keep]
			}
		}
		lastDepth = depth

		// Parse the line
		trimmed := strings.TrimSpace(line)

		// Skip tree drawing characters (and their variations)
		trimmed = strings.TrimPrefix(trimmed, "├── ")
		trimmed = strings.TrimPrefix(trimmed, "└── ")
		trimmed = strings.TrimPrefix(trimmed, "│   ")
		trimmed = strings.TrimPrefix(trimmed, "├──")
		trimmed = strings.TrimPrefix(trimmed, "└──")
		trimmed = strings.TrimPrefix(trimmed, "│")
		trimmed = strings.TrimSpace(trimmed)

		if trimmed == "" {
			continue
		}

		// Check for symlink syntax: name -> target
		isSymlink := false
		target := ""
		if parts := strings.Split(trimmed, " -> "); len(parts) == 2 {
			trimmed = parts[0]
			target = parts[1]
			isSymlink = true
		}

		// Check if it's a directory (ends with /)
		isDir := strings.HasSuffix(trimmed, "/")
		if isDir {
			trimmed = strings.TrimSuffix(trimmed, "/")
		}

		// Build full path
		fullPath := trimmed
		if len(dirStack) > 0 {
			fullPath = filepath.Join(append(dirStack, trimmed)...)
		}

		// Create entry
		entry := StructureEntry{
			Path:      fullPath,
			IsDir:     isDir,
			Mode:      0644,
			IsSymlink: isSymlink,
			Target:    target,
		}

		if isDir {
			entry.Mode = 0755
		}

		entries = append(entries, entry)

		// Update directory stack AFTER creating the entry
		if isDir {
			dirStack = append(dirStack, trimmed)
		}
	}

	return entries, nil
}

// CreateStructureOperation creates a directory structure from a definition
type CreateStructureOperation struct {
	id          OperationID
	desc        OperationDesc
	structure   string
	entries     []StructureEntry
	baseDir     string
	fileContent map[string][]byte
}

// NewCreateStructureOperation creates a new structure creation operation
func (s *SynthFS) NewCreateStructureOperation(structure string, baseDir string) (*CreateStructureOperation, error) {
	entries, err := ParseStructure(structure)
	if err != nil {
		return nil, err
	}

	id := s.idGen("create_structure", baseDir)
	return &CreateStructureOperation{
		id: id,
		desc: OperationDesc{
			Type: "create_structure",
			Path: baseDir,
			Details: map[string]interface{}{
				"structure":   structure,
				"entry_count": len(entries),
			},
		},
		structure:   structure,
		entries:     entries,
		baseDir:     baseDir,
		fileContent: make(map[string][]byte),
	}, nil
}

// WithFileContent sets content for a specific file in the structure
func (op *CreateStructureOperation) WithFileContent(path string, content []byte) *CreateStructureOperation {
	op.fileContent[path] = content
	return op
}

// ID returns the operation ID
func (op *CreateStructureOperation) ID() OperationID {
	return op.id
}

// Describe returns the operation description
func (op *CreateStructureOperation) Describe() OperationDesc {
	return op.desc
}

// Dependencies returns empty - no dependencies
func (op *CreateStructureOperation) Dependencies() []OperationID {
	return nil
}

// Conflicts returns empty - no conflicts
func (op *CreateStructureOperation) Conflicts() []OperationID {
	return nil
}

// Prerequisites returns prerequisites for the operation
func (op *CreateStructureOperation) Prerequisites() []core.Prerequisite {
	var prereqs []core.Prerequisite

	// Need base directory parent to exist
	if op.baseDir != "" && op.baseDir != "." {
		prereqs = append(prereqs, core.NewParentDirPrerequisite(op.baseDir))
	}

	return prereqs
}

// GetItem returns nil - no specific item
func (op *CreateStructureOperation) GetItem() FsItem {
	return nil
}

// SetDescriptionDetail sets a detail in the description
func (op *CreateStructureOperation) SetDescriptionDetail(key string, value interface{}) {
	if op.desc.Details == nil {
		op.desc.Details = make(map[string]interface{})
	}
	op.desc.Details[key] = value
}

// AddDependency adds a dependency
func (op *CreateStructureOperation) AddDependency(depID OperationID) {
	// Not implemented for this operation
}

// SetPaths sets source and destination paths
func (op *CreateStructureOperation) SetPaths(src, dst string) {
	op.baseDir = dst
	op.desc.Path = dst
}

// GetChecksum returns nil
func (op *CreateStructureOperation) GetChecksum(path string) *ChecksumRecord {
	return nil
}

// GetAllChecksums returns nil
func (op *CreateStructureOperation) GetAllChecksums() map[string]*ChecksumRecord {
	return nil
}

// ExecuteV2 is not implemented
func (op *CreateStructureOperation) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return fmt.Errorf("ExecuteV2 not implemented for CreateStructureOperation")
}

// ValidateV2 is not implemented
func (op *CreateStructureOperation) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return fmt.Errorf("ValidateV2 not implemented for CreateStructureOperation")
}

// Rollback is not implemented yet
func (op *CreateStructureOperation) Rollback(ctx context.Context, fsys FileSystem) error {
	// Would need to track all created files/dirs
	return fmt.Errorf("rollback not implemented for CreateStructureOperation")
}

// ReverseOps generates reverse operations
func (op *CreateStructureOperation) ReverseOps(ctx context.Context, fsys FileSystem, budget *core.BackupBudget) ([]Operation, *core.BackupData, error) {
	// Would create delete operations for all created items
	return nil, nil, fmt.Errorf("reverse ops not implemented for CreateStructureOperation")
}

// Execute performs the structure creation
func (op *CreateStructureOperation) Execute(ctx context.Context, fsys FileSystem) error {
	writeFS, ok := fsys.(WriteFS)
	if !ok {
		return fmt.Errorf("filesystem does not support write operations")
	}

	// Sort entries: directories first, then files, then symlinks
	// This ensures targets exist before symlinks are created
	sortedEntries := make([]StructureEntry, len(op.entries))
	copy(sortedEntries, op.entries)

	// Custom sort
	for i := 0; i < len(sortedEntries)-1; i++ {
		for j := i + 1; j < len(sortedEntries); j++ {
			// Directories come first
			if sortedEntries[j].IsDir && !sortedEntries[i].IsDir && !sortedEntries[i].IsSymlink {
				sortedEntries[i], sortedEntries[j] = sortedEntries[j], sortedEntries[i]
			}
			// Files come before symlinks
			if !sortedEntries[j].IsSymlink && sortedEntries[i].IsSymlink {
				sortedEntries[i], sortedEntries[j] = sortedEntries[j], sortedEntries[i]
			}
		}
	}

	// Create entries
	for _, entry := range sortedEntries {
		fullPath := entry.Path
		if op.baseDir != "" && op.baseDir != "." {
			fullPath = filepath.Join(op.baseDir, entry.Path)
		}

		if entry.IsSymlink {
			// Create symlink
			if fullFS, ok := fsys.(FullFileSystem); ok {
				// Ensure parent directory exists
				parent := filepath.Dir(fullPath)
				if parent != "." && parent != "/" {
					_ = writeFS.MkdirAll(parent, 0755)
				}

				// Use PathAwareFileSystem if available for secure symlink resolution
				var resolvedTarget string
				if pafs, ok := fsys.(*PathAwareFileSystem); ok {
					// Use centralized security-aware symlink resolution
					resolved, err := pafs.ResolveSymlinkTarget(fullPath, entry.Target)
					if err != nil {
						return fmt.Errorf("failed to resolve symlink target for %s -> %s: %w", fullPath, entry.Target, err)
					}
					resolvedTarget = resolved
				} else {
					// Fallback for non-PathAwareFileSystem (should not happen in practice)
					resolvedTarget = entry.Target
				}

				if err := fullFS.Symlink(resolvedTarget, fullPath); err != nil {
					return fmt.Errorf("failed to create symlink %s -> %s: %w", fullPath, resolvedTarget, err)
				}
			} else {
				return fmt.Errorf("filesystem does not support symlinks")
			}
		} else if entry.IsDir {
			// Create directory
			if err := writeFS.MkdirAll(fullPath, entry.Mode); err != nil {
				return fmt.Errorf("failed to create directory %s: %w", fullPath, err)
			}
		} else {
			// Create file
			// Ensure parent directory exists
			parent := filepath.Dir(fullPath)
			if parent != "." && parent != "/" {
				_ = writeFS.MkdirAll(parent, 0755)
			}

			// Get content if provided
			content := entry.Content
			// Try to find content by various path formats
			if customContent, ok := op.fileContent[entry.Path]; ok {
				content = customContent
			} else if customContent, ok := op.fileContent[fullPath]; ok {
				content = customContent
			} else {
				// Try relative path without the root directory
				relPath := entry.Path
				if idx := strings.Index(relPath, "/"); idx > 0 {
					relPath = relPath[idx+1:]
					if customContent, ok := op.fileContent[relPath]; ok {
						content = customContent
					}
				}
			}

			if err := writeFS.WriteFile(fullPath, content, entry.Mode); err != nil {
				return fmt.Errorf("failed to create file %s: %w", fullPath, err)
			}
		}
	}

	return nil
}

// Validate checks if the operation can be performed
func (op *CreateStructureOperation) Validate(ctx context.Context, fsys FileSystem) error {
	// Check if filesystem supports required operations
	if _, ok := fsys.(WriteFS); !ok {
		return fmt.Errorf("filesystem does not support write operations")
	}

	// Check for symlinks if needed
	hasSymlinks := false
	for _, entry := range op.entries {
		if entry.IsSymlink {
			hasSymlinks = true
			break
		}
	}

	if hasSymlinks {
		if _, ok := fsys.(FullFileSystem); !ok {
			return fmt.Errorf("filesystem does not support symlinks")
		}
	}

	return nil
}

// CreateStructure creates a directory structure from a string definition
func (s *SynthFS) CreateStructure(structure string) (Operation, error) {
	return s.NewCreateStructureOperation(structure, "")
}

// CreateStructureIn creates a directory structure in a specific base directory
func (s *SynthFS) CreateStructureIn(baseDir, structure string) (Operation, error) {
	return s.NewCreateStructureOperation(structure, baseDir)
}

// StructureBuilder provides a fluent interface for building directory structures
type StructureBuilder struct {
	structure   string
	baseDir     string
	fileContent map[string][]byte
}

// NewStructureBuilder creates a new structure builder
func NewStructureBuilder() *StructureBuilder {
	return &StructureBuilder{
		fileContent: make(map[string][]byte),
	}
}

// FromString sets the structure from a string definition
func (sb *StructureBuilder) FromString(structure string) *StructureBuilder {
	sb.structure = structure
	return sb
}

// InDirectory sets the base directory
func (sb *StructureBuilder) InDirectory(dir string) *StructureBuilder {
	sb.baseDir = dir
	return sb
}

// WithFile adds content for a specific file
func (sb *StructureBuilder) WithFile(path string, content []byte) *StructureBuilder {
	sb.fileContent[path] = content
	return sb
}

// WithTextFile adds text content for a specific file
func (sb *StructureBuilder) WithTextFile(path string, content string) *StructureBuilder {
	return sb.WithFile(path, []byte(content))
}

// Build creates the structure operation
func (sb *StructureBuilder) Build() (Operation, error) {
	op, err := New().NewCreateStructureOperation(sb.structure, sb.baseDir)
	if err != nil {
		return nil, err
	}

	// Add file content
	for path, content := range sb.fileContent {
		op.WithFileContent(path, content)
	}

	return op, nil
}

// Execute builds and executes the operation
func (sb *StructureBuilder) Execute(ctx context.Context, fs FileSystem) error {
	op, err := sb.Build()
	if err != nil {
		return err
	}
	return op.Execute(ctx, fs)
}
