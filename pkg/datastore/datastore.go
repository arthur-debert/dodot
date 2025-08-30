package datastore

import "github.com/arthur-debert/dodot/pkg/types"

// DataStore represents dodot's simplified storage interface.
// Phase 3: This interface now has only 5 operations instead of 20+.
// The simplicity is intentional - handlers contain logic, not the storage layer.
type DataStore interface {
	// CreateDataLink links a source file into the datastore structure.
	// Returns the path to the created link in the datastore.
	// This is step 1 for handlers that need to stage files.
	CreateDataLink(pack, handlerName, sourceFile string) (datastorePath string, err error)

	// CreateUserLink creates a user-visible symlink.
	// This is step 2 for the symlink handler to make files accessible.
	// Other handlers don't need this - their files are accessed via shell init.
	CreateUserLink(datastorePath, userPath string) error

	// RunAndRecord executes a command and records completion with a sentinel.
	// This is idempotent - if the sentinel exists, the command is not re-run.
	// Used by provisioning handlers (install, homebrew) to track completion.
	RunAndRecord(pack, handlerName, command, sentinel string) error

	// HasSentinel checks if an operation has been completed.
	// This enables idempotent operations and status reporting.
	HasSentinel(pack, handlerName, sentinel string) (bool, error)

	// RemoveState removes all state for a handler in a pack.
	// This is used for cleanup/uninstall operations.
	RemoveState(pack, handlerName string) error

	// Legacy methods still needed during transition
	// TODO: Remove these in final cleanup
	Link(pack, sourceFile string) (intermediateLinkPath string, err error)
	Unlink(pack, sourceFile string) error
	AddToPath(pack, dirPath string) error
	AddToShellProfile(pack, scriptPath string) error
	RecordProvisioning(pack, sentinelName, checksum string) error
	NeedsProvisioning(pack, sentinelName, checksum string) (bool, error)
	GetStatus(pack, sourceFile string) (types.Status, error)
	GetSymlinkStatus(pack, sourceFile string) (types.Status, error)
	GetPathStatus(pack, dirPath string) (types.Status, error)
	GetShellProfileStatus(pack, scriptPath string) (types.Status, error)
	GetProvisioningStatus(pack, sentinelName, currentChecksum string) (types.Status, error)
	GetBrewStatus(pack, brewfilePath, currentChecksum string) (types.Status, error)
	DeleteProvisioningState(packName, handlerName string) error
	GetProvisioningHandlers(packName string) ([]string, error)
	ListProvisioningState(packName string) (map[string][]string, error)
	StoreState(packName, handlerName string, state interface{}) error
	GetState(packName, handlerName string) (interface{}, error)
}
