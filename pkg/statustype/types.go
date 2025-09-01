// Package statustype provides common types and utilities for status operations
// without importing other dodot packages, preventing circular dependencies.
package statustype

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetPackStatusFunc is a function type for getting pack status to avoid circular imports
type GetPackStatusFunc func(packName, dotfilesRoot string, fs types.FS) ([]types.DisplayPack, error)
