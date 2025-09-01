package pack

import (
	"os"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/types"
)

// Pack function wrappers for standardizing the pipeline interface.
// These functions receive a pack object and delegate to the pack methods,
// providing a consistent function-based interface for all pack operations.

// GetPackFilePath returns the full path to a file within the pack
func GetPackFilePath(pack *types.Pack, filename string) string {
	return pack.GetFilePath(filename)
}

// PackFileExists checks if a file exists within the pack
func PackFileExists(pack *types.Pack, fs types.FS, filename string) (bool, error) {
	return pack.FileExists(fs, filename)
}

// CreatePackFile creates a file within the pack with the given content
func CreatePackFile(pack *types.Pack, fs types.FS, filename, content string) error {
	return pack.CreateFile(fs, filename, content)
}

// ReadPackFile reads a file from within the pack
func ReadPackFile(pack *types.Pack, fs types.FS, filename string) ([]byte, error) {
	return pack.ReadFile(fs, filename)
}

// CreatePackDirectory creates a directory within the pack
func CreatePackDirectory(pack *types.Pack, fs types.FS, dirname string) error {
	return pack.CreateDirectory(fs, dirname)
}

// CreatePackFileWithMode creates a file within the pack with specific permissions
func CreatePackFileWithMode(pack *types.Pack, fs types.FS, filename, content string, mode os.FileMode) error {
	return pack.CreateFileWithMode(fs, filename, content, mode)
}

// AdoptPackFile moves an external file into the pack and returns the destination path
func AdoptPackFile(pack *types.Pack, fs types.FS, externalPath, internalPath string, force bool) (string, error) {
	return pack.AdoptFile(fs, externalPath, internalPath, force)
}

// CreatePackIgnoreFile creates a .dodotignore file in the pack
func CreatePackIgnoreFile(pack *types.Pack, fs types.FS, cfg *config.Config) error {
	return pack.CreateIgnoreFile(fs, cfg)
}

// PackHasIgnoreFile checks if the pack has an ignore file
func PackHasIgnoreFile(pack *types.Pack, fs types.FS, cfg *config.Config) (bool, error) {
	return pack.HasIgnoreFile(fs, cfg)
}

// IsPackHandlerProvisioned checks if a specific handler has been provisioned for this pack
func IsPackHandlerProvisioned(pack *types.Pack, store types.DataStore, handlerName string) (bool, error) {
	return pack.IsHandlerProvisioned(store, handlerName)
}

// GetPackProvisionedHandlers returns a list of all handlers that have been provisioned for this pack
func GetPackProvisionedHandlers(pack *types.Pack, store types.DataStore) ([]string, error) {
	return pack.GetProvisionedHandlers(store)
}
