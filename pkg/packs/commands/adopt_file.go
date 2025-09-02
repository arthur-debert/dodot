package commands

import (
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/types"
)

// AdoptPackFile moves an external file into the pack and returns the destination path
func AdoptPackFile(pack *types.Pack, fs types.FS, externalPath, internalPath string, force bool) (string, error) {
	destPath := GetPackFilePath(pack, internalPath)

	// Ensure destination directory exists
	destDir := filepath.Dir(destPath)
	if err := fs.MkdirAll(destDir, 0755); err != nil {
		return "", err
	}

	// Check if destination already exists
	if _, err := fs.Stat(destPath); err == nil {
		if !force {
			return "", fmt.Errorf("destination already exists: %s (use --force to overwrite)", destPath)
		}
		// Remove existing file if force is enabled
		if err := fs.Remove(destPath); err != nil {
			return "", fmt.Errorf("failed to remove existing destination: %w", err)
		}
	}

	// Move the file
	if err := fs.Rename(externalPath, destPath); err != nil {
		return "", fmt.Errorf("failed to move file: %w", err)
	}

	return destPath, nil
}
