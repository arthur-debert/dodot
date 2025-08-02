// Package core provides prerequisite interfaces and types for the synthfs system.
package core

import (
	"fmt"
	"path/filepath"
)

// Prerequisite represents a condition that must be met before an operation executes
type Prerequisite interface {
	Type() string                    // "parent_dir", "no_conflict", "source_exists"
	Path() string                    // Path this prerequisite relates to
	Validate(fsys interface{}) error // Check if prerequisite is satisfied
}

// PrerequisiteResolver can create operations to satisfy prerequisites
type PrerequisiteResolver interface {
	CanResolve(prereq Prerequisite) bool
	Resolve(prereq Prerequisite) ([]interface{}, error) // Returns operations
}

// ParentDirPrerequisite ensures that the parent directory exists
type ParentDirPrerequisite struct {
	path string
}

// NewParentDirPrerequisite creates a prerequisite that ensures parent directory exists
func NewParentDirPrerequisite(path string) Prerequisite {
	return &ParentDirPrerequisite{
		path: path,
	}
}

func (p *ParentDirPrerequisite) Type() string {
	return "parent_dir"
}

func (p *ParentDirPrerequisite) Path() string {
	// Return the parent directory path, not the target path
	dir := filepath.Dir(p.path)
	if dir == "." || dir == p.path {
		return ""
	}
	return dir
}

func (p *ParentDirPrerequisite) Validate(fsys interface{}) error {
	parentPath := p.Path()
	if parentPath == "" || parentPath == "/" || parentPath == "." {
		return nil // Root or current directory - no parent needed
	}

	// Try different filesystem interfaces
	if stat, ok := fsys.(interface {
		Stat(string) (interface{}, error)
	}); ok {
		if info, err := stat.Stat(parentPath); err != nil {
			return fmt.Errorf("parent directory %s does not exist", parentPath)
		} else {
			// Check if it's actually a directory
			if dirChecker, ok := info.(interface{ IsDir() bool }); ok {
				if !dirChecker.IsDir() {
					return fmt.Errorf("parent path %s exists but is not a directory", parentPath)
				}
			}
		}
		return nil
	}

	return fmt.Errorf("filesystem does not support Stat operation")
}

// NoConflictPrerequisite ensures that the target path doesn't already exist
type NoConflictPrerequisite struct {
	path string
}

// NewNoConflictPrerequisite creates a prerequisite that ensures no conflict with existing files
func NewNoConflictPrerequisite(path string) Prerequisite {
	return &NoConflictPrerequisite{
		path: path,
	}
}

func (p *NoConflictPrerequisite) Type() string {
	return "no_conflict"
}

func (p *NoConflictPrerequisite) Path() string {
	return p.path
}

func (p *NoConflictPrerequisite) Validate(fsys interface{}) error {
	// Try different filesystem interfaces
	if stat, ok := fsys.(interface {
		Stat(string) (interface{}, error)
	}); ok {
		if _, err := stat.Stat(p.path); err == nil {
			return fmt.Errorf("path %s already exists", p.path)
		}
		// If Stat returns an error, the path doesn't exist, which is what we want
		return nil
	}

	return fmt.Errorf("filesystem does not support Stat operation")
}

// SourceExistsPrerequisite ensures that a source path exists
type SourceExistsPrerequisite struct {
	path string
}

// NewSourceExistsPrerequisite creates a prerequisite that ensures source exists
func NewSourceExistsPrerequisite(path string) Prerequisite {
	return &SourceExistsPrerequisite{
		path: path,
	}
}

func (p *SourceExistsPrerequisite) Type() string {
	return "source_exists"
}

func (p *SourceExistsPrerequisite) Path() string {
	return p.path
}

func (p *SourceExistsPrerequisite) Validate(fsys interface{}) error {
	// Try different filesystem interfaces
	if stat, ok := fsys.(interface {
		Stat(string) (interface{}, error)
	}); ok {
		if _, err := stat.Stat(p.path); err != nil {
			return fmt.Errorf("source path %s does not exist", p.path)
		}
		return nil
	}

	return fmt.Errorf("filesystem does not support Stat operation")
}
