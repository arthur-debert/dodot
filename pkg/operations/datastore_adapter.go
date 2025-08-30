package operations

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/types"
)

// DataStoreAdapter implements SimpleDataStore using the existing DataStore.
// This is a temporary adapter for phase 1 that proves the concept works.
// In phase 3, the DataStore interface will be simplified to these 4 methods.
type DataStoreAdapter struct {
	store types.DataStore
	fs    types.FS
}

// NewDataStoreAdapter creates a new adapter.
func NewDataStoreAdapter(store types.DataStore, fs types.FS) *DataStoreAdapter {
	return &DataStoreAdapter{
		store: store,
		fs:    fs,
	}
}

// CreateDataLink implements SimpleDataStore.CreateDataLink.
// This maps to different DataStore methods based on handler type.
func (a *DataStoreAdapter) CreateDataLink(pack, handlerName, sourceFile string) (string, error) {
	switch handlerName {
	case "symlink":
		// For symlink handler, use the Link method which creates intermediate links
		return a.store.Link(pack, sourceFile)

	case "path":
		// For path handler, use AddToPath
		// The existing method doesn't return a path, so we construct it
		err := a.store.AddToPath(pack, sourceFile)
		if err != nil {
			return "", err
		}
		// Return constructed path for consistency
		// In real implementation, AddToPath would return the created link path
		return filepath.Join("~/.local/share/dodot/data", pack, "path", sourceFile), nil

	case "shell", "shell_profile":
		// For shell handler, use AddToShellProfile
		err := a.store.AddToShellProfile(pack, sourceFile)
		if err != nil {
			return "", err
		}
		// Return constructed path for consistency
		return filepath.Join("~/.local/share/dodot/data", pack, "shell", filepath.Base(sourceFile)), nil

	default:
		return "", fmt.Errorf("unsupported handler for CreateDataLink: %s", handlerName)
	}
}

// CreateUserLink implements SimpleDataStore.CreateUserLink.
// This creates the final user-visible symlink.
func (a *DataStoreAdapter) CreateUserLink(datastorePath, userPath string) error {
	// Expand home directory if needed
	if userPath[0] == '~' {
		home, err := os.UserHomeDir()
		if err != nil {
			return fmt.Errorf("failed to get home directory: %w", err)
		}
		userPath = filepath.Join(home, userPath[1:])
	}

	// Ensure parent directory exists
	parentDir := filepath.Dir(userPath)
	if err := a.fs.MkdirAll(parentDir, 0755); err != nil {
		return fmt.Errorf("failed to create parent directory: %w", err)
	}

	// Remove existing file/link if present
	if err := a.fs.Remove(userPath); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to remove existing file: %w", err)
	}

	// Create the symlink
	if err := a.fs.Symlink(datastorePath, userPath); err != nil {
		return fmt.Errorf("failed to create symlink: %w", err)
	}

	return nil
}

// RunAndRecord implements SimpleDataStore.RunAndRecord.
// This executes a command and records completion.
func (a *DataStoreAdapter) RunAndRecord(pack, handlerName, command, sentinel string) error {
	// Check if already run
	if exists, _ := a.HasSentinel(pack, handlerName, sentinel); exists {
		return nil // Idempotent - don't re-run
	}

	// Execute the command
	// In real implementation, this would handle working directory, environment, etc.
	cmd := exec.Command("sh", "-c", command)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("command failed: %w\nOutput: %s", err, output)
	}

	// Record completion using existing DataStore method
	// The checksum in sentinel is used as both sentinel name and checksum
	return a.store.RecordProvisioning(pack, sentinel, sentinel)
}

// HasSentinel implements SimpleDataStore.HasSentinel.
// This checks if an operation has been completed.
func (a *DataStoreAdapter) HasSentinel(pack, handlerName, sentinel string) (bool, error) {
	// Use NeedsProvisioning with inverted logic
	// If it doesn't need provisioning, then the sentinel exists
	needs, err := a.store.NeedsProvisioning(pack, sentinel, sentinel)
	if err != nil {
		return false, err
	}
	return !needs, nil
}
