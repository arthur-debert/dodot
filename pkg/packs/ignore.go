package packs

import (
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/rs/zerolog"
)

// IgnoreChecker provides unified interface for checking if packs or files should be ignored
type IgnoreChecker struct {
	logger zerolog.Logger
}

// NewIgnoreChecker creates a new IgnoreChecker instance
func NewIgnoreChecker() *IgnoreChecker {
	return &IgnoreChecker{
		logger: logging.GetLogger("packs.ignore"),
	}
}

// ShouldIgnorePackDirectory checks if a pack directory should be ignored due to .dodotignore file
func (ic *IgnoreChecker) ShouldIgnorePackDirectory(packPath string) bool {
	ignoreFilePath := filepath.Join(packPath, ".dodotignore")
	if _, err := os.Stat(ignoreFilePath); err == nil {
		ic.logger.Debug().
			Str("pack", filepath.Base(packPath)).
			Msg("Pack ignored due to .dodotignore file")
		return true
	}
	return false
}

// ShouldIgnoreDirectoryDuringTraversal checks if a directory should be skipped during file traversal
// This is used when walking through pack contents to find files
func (ic *IgnoreChecker) ShouldIgnoreDirectoryDuringTraversal(dirPath string, relPath string) bool {
	ignoreFilePath := filepath.Join(dirPath, ".dodotignore")
	if _, err := os.Stat(ignoreFilePath); err == nil {
		ic.logger.Debug().
			Str("dir", relPath).
			Msg("Skipping directory with .dodotignore during traversal")
		return true
	}
	return false
}

// HasIgnoreFile checks if a directory contains a .dodotignore file
func (ic *IgnoreChecker) HasIgnoreFile(dirPath string) bool {
	ignoreFilePath := filepath.Join(dirPath, ".dodotignore")
	if _, err := os.Stat(ignoreFilePath); err == nil {
		return true
	}
	return false
}

// ShouldIgnorePack checks if a pack should be ignored by checking for a .dodotignore file
// This function maintains backward compatibility with existing code
func ShouldIgnorePack(packPath string) bool {
	checker := NewIgnoreChecker()
	return checker.ShouldIgnorePackDirectory(packPath)
}

// ShouldIgnoreDirectoryTraversal checks if a directory should be skipped during traversal
// This function provides a convenient interface for file walking operations
func ShouldIgnoreDirectoryTraversal(dirPath string, relPath string) bool {
	checker := NewIgnoreChecker()
	return checker.ShouldIgnoreDirectoryDuringTraversal(dirPath, relPath)
}
