package core

// OperationMetadata provides basic information about an operation
type OperationMetadata interface {
	ID() OperationID
	Describe() OperationDesc
}

// DependencyAware provides dependency and conflict information
type DependencyAware interface {
	Dependencies() []OperationID
	Conflicts() []OperationID
}

// Note: Executable interface will be defined in the main synthfs package
// because it depends on filesystem.FileSystem which would create a circular dependency

// ExecutableV2 defines execution capabilities using ExecutionContext
type ExecutableV2 interface {
	ExecuteV2(ctx interface{}, execCtx *ExecutionContext, fsys interface{}) error
	ValidateV2(ctx interface{}, execCtx *ExecutionContext, fsys interface{}) error
}

// OperationFactory creates operations based on type and item
// Note: The actual Operation interface is defined in the main synthfs package
type OperationFactory interface {
	CreateOperation(id OperationID, opType string, path string) (interface{}, error)
	SetItemForOperation(op interface{}, item interface{}) error
}

// BatchOperationInterface defines the core operation interface for the batch package
type BatchOperationInterface interface {
	OperationMetadata
	DependencyAware
	ExecutableV2
	// Additional methods needed by batch
	Validate(ctx interface{}, fsys interface{}) error
	SetDescriptionDetail(key string, value interface{})
	AddDependency(depID OperationID)
	SetPaths(src, dst string)
	GetItem() interface{}
}

// FilesystemInterface defines the core filesystem interface for the batch package
type FilesystemInterface interface {
	// Read operations
	Stat(name string) (interface{}, error)
	Open(name string) (interface{}, error)
	// Write operations (for FullFileSystem)
	WriteFile(name string, data []byte, perm interface{}) error
	MkdirAll(path string, perm interface{}) error
	Remove(name string) error
	RemoveAll(name string) error
	Rename(oldpath, newpath string) error
	Symlink(oldname, newname string) error
}

// BatchInterface defines the interface for batch orchestration
type BatchInterface interface {
	Operations() []interface{}
	CreateDir(path string, mode ...interface{}) (interface{}, error)
	CreateFile(path string, content []byte, mode ...interface{}) (interface{}, error)
	Copy(src, dst string) (interface{}, error)
	Move(src, dst string) (interface{}, error)
	Delete(path string) (interface{}, error)
	CreateSymlink(target, linkPath string) (interface{}, error)
	WithFileSystem(fs interface{}) interface{}
	WithContext(ctx interface{}) interface{}
}
