package operations

import (
	"context"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// Operation represents a filesystem operation with minimal coupling.
// This interface uses interface{} types to avoid circular dependencies.
type Operation interface {
	// Core metadata
	ID() core.OperationID
	Describe() core.OperationDesc

	// Dependencies
	Dependencies() []core.OperationID
	Conflicts() []core.OperationID
	AddDependency(depID core.OperationID)
	Prerequisites() []core.Prerequisite

	// Item management - returns interface{} to avoid coupling to specific types
	GetItem() interface{}
	SetItem(item interface{})

	// Path management for operations like copy/move
	GetPaths() (src, dst string)
	SetPaths(src, dst string)

	// Checksum management - returns interface{} instead of ChecksumRecord
	GetChecksum(path string) interface{}
	GetAllChecksums() map[string]interface{}
	SetChecksum(path string, checksum interface{})

	// Description details
	SetDescriptionDetail(key string, value interface{})

	// Execution methods - take interface{} for filesystem to avoid coupling
	Execute(ctx context.Context, fsys interface{}) error
	Validate(ctx context.Context, fsys interface{}) error
	Rollback(ctx context.Context, fsys interface{}) error

	// ExecuteV2 for new execution context pattern
	ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error
	ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error

	// Reverse operations - returns interface{} for operations and backup data
	ReverseOps(ctx context.Context, fsys interface{}, budget interface{}) ([]interface{}, interface{}, error)
}

// ItemInterface represents a filesystem item to be created
type ItemInterface interface {
	Path() string
	Type() string
}

// ChecksumInterface represents a checksum record
type ChecksumInterface interface {
	GetPath() string
	GetMD5() string
	GetSize() int64
}
