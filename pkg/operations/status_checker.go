package operations

import (
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// DataStoreStatusChecker implements StatusChecker using the datastore
type DataStoreStatusChecker struct {
	dataStore datastore.DataStore
	fs        types.FS
	paths     types.Pather
}

// NewDataStoreStatusChecker creates a new status checker that uses the datastore
func NewDataStoreStatusChecker(dataStore datastore.DataStore, fs types.FS, pathsInstance types.Pather) *DataStoreStatusChecker {
	return &DataStoreStatusChecker{
		dataStore: dataStore,
		fs:        fs,
		paths:     pathsInstance,
	}
}

// HasDataLink checks if a data link exists in the datastore
func (d *DataStoreStatusChecker) HasDataLink(packName, handlerName, relativePath string) (bool, error) {
	// Get the path where the link should exist
	// Need to type assert to paths.Paths to access PackHandlerDir
	pathsInstance, ok := d.paths.(paths.Paths)
	if !ok {
		// Fallback: construct path manually if not paths.Paths
		linkPath := filepath.Join(d.paths.DataDir(), "deployed", packName, handlerName)
		targetPath := filepath.Join(linkPath, relativePath)

		// Check if the file exists
		_, err := d.fs.Stat(targetPath)
		if err != nil {
			// If file doesn't exist, that's expected - return false
			return false, nil
		}
		return true, nil
	}

	linkPath := pathsInstance.PackHandlerDir(packName, handlerName)
	targetPath := filepath.Join(linkPath, relativePath)

	// Check if the file exists
	_, err := d.fs.Stat(targetPath)
	if err != nil {
		// If file doesn't exist, that's expected - return false
		return false, nil
	}

	// File exists
	return true, nil
}

// HasSentinel checks if a sentinel exists for tracking operation completion
func (d *DataStoreStatusChecker) HasSentinel(packName, handlerName, sentinel string) (bool, error) {
	return d.dataStore.HasSentinel(packName, handlerName, sentinel)
}

// GetMetadata retrieves metadata for future extensibility
func (d *DataStoreStatusChecker) GetMetadata(packName, handlerName, key string) (string, error) {
	// TODO: Implement when datastore supports metadata
	return "", nil
}
