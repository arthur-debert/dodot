package core

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/types"
)

// Common validators for command execution

// ValidateSinglePack ensures exactly one pack is specified
func ValidateSinglePack(packs []types.Pack, opts CommandExecuteOptions) error {
	// Check for empty pack name
	if len(opts.PackNames) > 0 && opts.PackNames[0] == "" {
		return fmt.Errorf("pack name cannot be empty")
	}
	if len(packs) != 1 {
		return fmt.Errorf("this command requires exactly one pack, got %d", len(packs))
	}
	return nil
}

// ValidateSinglePackName ensures exactly one pack name is provided
func ValidateSinglePackName(packs []types.Pack, opts CommandExecuteOptions) error {
	if len(opts.PackNames) != 1 {
		return fmt.Errorf("this command requires exactly one pack name, got %d", len(opts.PackNames))
	}
	return nil
}

// ValidatePackDoesNotExist ensures the pack doesn't already exist (for init)
func ValidatePackDoesNotExist(packs []types.Pack, opts CommandExecuteOptions) error {
	if len(opts.PackNames) == 0 {
		return fmt.Errorf("pack name is required")
	}

	packName := opts.PackNames[0]
	packPath := filepath.Join(opts.DotfilesRoot, packName)

	fs := opts.FileSystem
	if fs == nil {
		// This shouldn't happen in practice but let's be safe
		return fmt.Errorf("filesystem not initialized")
	}

	if _, err := fs.Stat(packPath); err == nil {
		return fmt.Errorf("pack '%s' already exists at %s", packName, packPath)
	} else if !os.IsNotExist(err) {
		return fmt.Errorf("error checking pack existence: %w", err)
	}

	return nil
}

// ValidateFileExists ensures a file path is provided and exists (for adopt)
func ValidateFileExists(packs []types.Pack, opts CommandExecuteOptions) error {
	// Check if file path is provided in options
	filePath, ok := opts.Options["file"].(string)
	if !ok || filePath == "" {
		return fmt.Errorf("file path is required")
	}

	fs := opts.FileSystem
	if fs == nil {
		return fmt.Errorf("filesystem not initialized")
	}

	// Check if file exists
	info, err := fs.Stat(filePath)
	if err != nil {
		if os.IsNotExist(err) {
			return fmt.Errorf("file does not exist: %s", filePath)
		}
		return fmt.Errorf("error checking file: %w", err)
	}

	// Ensure it's a regular file
	if info.IsDir() {
		return fmt.Errorf("path is a directory, not a file: %s", filePath)
	}

	return nil
}

// ValidatePacksExist ensures all specified packs exist
func ValidatePacksExist(packs []types.Pack, opts CommandExecuteOptions) error {
	if len(packs) == 0 && len(opts.PackNames) > 0 {
		return fmt.Errorf("no valid packs found matching: %v", opts.PackNames)
	}
	return nil
}

// ValidateAtLeastOnePack ensures at least one pack is specified
func ValidateAtLeastOnePack(packs []types.Pack, opts CommandExecuteOptions) error {
	if len(packs) == 0 {
		return fmt.Errorf("at least one pack is required")
	}
	return nil
}
