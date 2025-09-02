package operations

import (
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/types"
)

// GetPackFilePath returns the full path to a file within the pack
func GetPackFilePath(pack *types.Pack, filename string) string {
	return filepath.Join(pack.Path, filename)
}

// PackFileExists checks if a file exists within the pack
func PackFileExists(pack *types.Pack, fs types.FS, filename string) (bool, error) {
	path := GetPackFilePath(pack, filename)
	_, err := fs.Stat(path)
	if err != nil {
		// Check if it's a "not found" error
		if os.IsNotExist(err) {
			return false, nil
		}
		// For other errors (permission denied, etc.), return the error
		return false, err
	}
	return true, nil
}

// CreatePackFile creates a file within the pack with the given content
func CreatePackFile(pack *types.Pack, fs types.FS, filename, content string) error {
	path := GetPackFilePath(pack, filename)
	return fs.WriteFile(path, []byte(content), 0644)
}

// ReadPackFile reads a file from within the pack
func ReadPackFile(pack *types.Pack, fs types.FS, filename string) ([]byte, error) {
	path := GetPackFilePath(pack, filename)
	return fs.ReadFile(path)
}

// CreatePackDirectory creates a directory within the pack
func CreatePackDirectory(pack *types.Pack, fs types.FS, dirname string) error {
	path := GetPackFilePath(pack, dirname)
	return fs.MkdirAll(path, 0755)
}

// CreatePackFileWithMode creates a file within the pack with specific permissions
func CreatePackFileWithMode(pack *types.Pack, fs types.FS, filename, content string, mode os.FileMode) error {
	path := GetPackFilePath(pack, filename)
	// Ensure parent directory exists
	dir := filepath.Dir(path)
	if err := fs.MkdirAll(dir, 0755); err != nil {
		return err
	}
	return fs.WriteFile(path, []byte(content), mode)
}
