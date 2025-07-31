package execution

import (
	"fmt"
	"io/fs"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// DefaultPrerequisiteResolver resolves common prerequisites like parent directories
type DefaultPrerequisiteResolver struct {
	opFactory core.OperationFactory
	logger    core.Logger
}

// NewDefaultPrerequisiteResolver creates a new prerequisite resolver
func NewDefaultPrerequisiteResolver(opFactory core.OperationFactory) *DefaultPrerequisiteResolver {
	return &DefaultPrerequisiteResolver{
		opFactory: opFactory,
		logger:    nil,
	}
}

// NewPrerequisiteResolver creates a new prerequisite resolver with factory and logger
func NewPrerequisiteResolver(opFactory core.OperationFactory, logger core.Logger) *DefaultPrerequisiteResolver {
	return &DefaultPrerequisiteResolver{
		opFactory: opFactory,
		logger:    logger,
	}
}

// CanResolve checks if this resolver can handle the given prerequisite
func (r *DefaultPrerequisiteResolver) CanResolve(prereq core.Prerequisite) bool {
	switch prereq.Type() {
	case "parent_dir":
		return true
	default:
		return false
	}
}

// Resolve creates operations to satisfy the given prerequisite
func (r *DefaultPrerequisiteResolver) Resolve(prereq core.Prerequisite) ([]interface{}, error) {
	switch prereq.Type() {
	case "parent_dir":
		return r.resolveParentDir(prereq)
	default:
		return nil, fmt.Errorf("unsupported prerequisite type: %s", prereq.Type())
	}
}

// resolveParentDir creates a directory creation operation for parent directory prerequisite
func (r *DefaultPrerequisiteResolver) resolveParentDir(prereq core.Prerequisite) ([]interface{}, error) {
	path := prereq.Path()
	
	// Skip if it's root or current directory
	if path == "" || path == "." || path == "/" {
		return nil, nil
	}
	
	// Create a unique operation ID for the parent directory creation
	opID := core.OperationID(fmt.Sprintf("auto_mkdir_%s", generatePathID(path)))
	
	// Create directory operation using the factory
	op, err := r.opFactory.CreateOperation(opID, "create_directory", path)
	if err != nil {
		return nil, fmt.Errorf("failed to create directory operation for %s: %w", path, err)
	}
	
	// Create directory item using the same approach as batch
	dirItem := &DirectoryItem{
		path: path,
		mode: fs.FileMode(0755), // Default directory permissions
	}
	
	// Set the item on the operation
	if err := r.opFactory.SetItemForOperation(op, dirItem); err != nil {
		return nil, fmt.Errorf("failed to set item for directory operation: %w", err)
	}
	
	return []interface{}{op}, nil
}

// DirectoryItem represents a directory to be created
type DirectoryItem struct {
	path string
	mode fs.FileMode
}

// Path returns the directory path
func (d *DirectoryItem) Path() string {
	return d.path
}

// Type returns the item type
func (d *DirectoryItem) Type() string {
	return "directory"
}

// Mode returns the directory permissions
func (d *DirectoryItem) Mode() fs.FileMode {
	return d.mode
}

// IsDir returns true for directory items
func (d *DirectoryItem) IsDir() bool {
	return true
}

// generatePathID creates a safe ID from a path
func generatePathID(path string) string {
	// Replace path separators and special characters
	result := path
	result = replaceAll(result, "/", "_")
	result = replaceAll(result, "\\", "_")
	result = replaceAll(result, ":", "_")
	result = replaceAll(result, " ", "_")
	return result
}

// replaceAll is a simple string replacement function
func replaceAll(s, old, new string) string {
	result := ""
	for i := 0; i < len(s); {
		if i+len(old) <= len(s) && s[i:i+len(old)] == old {
			result += new
			i += len(old)
		} else {
			result += string(s[i])
			i++
		}
	}
	return result
}

// Note: NewPrerequisiteResolver is defined above and should be used to create prerequisite resolvers