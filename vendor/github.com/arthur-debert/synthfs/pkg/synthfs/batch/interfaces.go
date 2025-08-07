package batch

import (
	"context"
	"io/fs"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// Batch represents a collection of operations that can be validated and executed as a unit.
type Batch interface {
	// Operation management
	Operations() []interface{}

	// Operation creation methods
	CreateDir(path string, mode fs.FileMode, metadata ...map[string]interface{}) (interface{}, error)
	CreateFile(path string, content []byte, mode fs.FileMode, metadata ...map[string]interface{}) (interface{}, error)
	Copy(src, dst string, metadata ...map[string]interface{}) (interface{}, error)
	Move(src, dst string, metadata ...map[string]interface{}) (interface{}, error)
	Delete(path string, metadata ...map[string]interface{}) (interface{}, error)
	CreateSymlink(target, linkPath string, metadata ...map[string]interface{}) (interface{}, error)
	CreateArchive(archivePath string, format interface{}, sources []string, metadata ...map[string]interface{}) (interface{}, error)
	Unarchive(archivePath, extractPath string, metadata ...map[string]interface{}) (interface{}, error)
	UnarchiveWithPatterns(archivePath, extractPath string, patterns []string, metadata ...map[string]interface{}) (interface{}, error)

	// Configuration
	WithFileSystem(fs interface{}) Batch
	WithContext(ctx context.Context) Batch
	WithRegistry(registry core.OperationFactory) Batch
	WithLogger(logger core.Logger) Batch

	// Metadata management
	WithMetadata(metadata map[string]interface{}) Batch

	// Execution
	Run() (interface{}, error)
	RunWithOptions(opts interface{}) (interface{}, error)
	RunRestorable() (interface{}, error)
	RunRestorableWithBudget(maxBackupMB int) (interface{}, error)
}

// Result represents the outcome of executing a batch of operations
type Result interface {
	IsSuccess() bool
	GetOperations() []interface{}
	GetRestoreOps() []interface{}
	GetDuration() interface{}
	GetError() error
	GetBudget() interface{}
	GetRollback() interface{}
	GetMetadata() map[string]interface{}
}
