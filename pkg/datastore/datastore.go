package datastore

import "github.com/arthur-debert/dodot/pkg/types"

// DataStore manages dodot's internal state on the filesystem.
// This interface is also duplicated in pkg/types to avoid circular dependencies.
type DataStore interface {
	// Link creates the internal double-link structure for a file.
	// It returns the path to the intermediate link, which should be the
	// target for the final user-facing symlink.
	Link(pack, sourceFile string) (intermediateLinkPath string, err error)

	// Unlink removes the internal link for a file.
	Unlink(pack, sourceFile string) error

	// AddToPath makes a directory available to the shell's PATH.
	AddToPath(pack, dirPath string) error

	// AddToShellProfile makes a script available to be sourced.
	AddToShellProfile(pack, scriptPath string) error

	// RecordProvisioning marks a provisioning action as complete.
	RecordProvisioning(pack, sentinelName, checksum string) error

	// NeedsProvisioning checks if a provisioning action needs to run.
	NeedsProvisioning(pack, sentinelName, checksum string) (bool, error)

	// GetStatus returns the status of a specific link or resource.
	// This is a generic method that defaults to symlink handler behavior.
	GetStatus(pack, sourceFile string) (types.Status, error)

	// Handler-specific status methods
	GetSymlinkStatus(pack, sourceFile string) (types.Status, error)
	GetPathStatus(pack, dirPath string) (types.Status, error)
	GetShellProfileStatus(pack, scriptPath string) (types.Status, error)
	GetProvisioningStatus(pack, sentinelName, currentChecksum string) (types.Status, error)
	GetBrewStatus(pack, brewfilePath, currentChecksum string) (types.Status, error)

	// State removal methods

	// DeleteProvisioningState removes all provisioning state for a handler in a pack.
	// It only removes state for provisioning handlers (homebrew, provision).
	// Returns nil if the directory doesn't exist.
	DeleteProvisioningState(packName, handlerName string) error

	// GetProvisioningHandlers returns list of handlers that have provisioning state
	// for the given pack. Only returns handlers that actually have state on disk.
	GetProvisioningHandlers(packName string) ([]string, error)

	// ListProvisioningState returns details about what provisioning state exists.
	// The returned map has handler names as keys and lists of state file names as values.
	// Useful for dry-run operations to show what would be removed.
	ListProvisioningState(packName string) (map[string][]string, error)
}
