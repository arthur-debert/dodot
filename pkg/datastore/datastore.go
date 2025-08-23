package datastore

import "github.com/arthur-debert/dodot/pkg/types"

// DataStore manages dodot's internal state on the filesystem.
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
	GetStatus(pack, sourceFile string) (types.Status, error)
}
