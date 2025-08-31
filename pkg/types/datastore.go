package types

// DataStore represents dodot's simplified storage interface.
// This interface has only 5 operations instead of the previous 20+.
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

	// Query methods for checking handler state

	// HasHandlerState checks if any state exists for a handler in a pack.
	// This is useful for determining if a handler has been used/provisioned.
	HasHandlerState(pack, handlerName string) (bool, error)

	// ListPackHandlers returns a list of all handlers that have state for a given pack.
	// This helps identify which handlers have been used in a pack.
	ListPackHandlers(pack string) ([]string, error)

	// ListHandlerSentinels returns all sentinel files for a specific handler in a pack.
	// This provides detailed information about what operations have been completed.
	ListHandlerSentinels(pack, handlerName string) ([]string, error)
}
