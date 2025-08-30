package datastore

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

type filesystemDataStore struct {
	fs    types.FS
	paths paths.Paths
}

// New creates a new DataStore instance that interacts with the filesystem.
func New(fs types.FS, paths paths.Paths) DataStore {
	return &filesystemDataStore{
		fs:    fs,
		paths: paths,
	}
}

// CreateDataLink implements the simplified DataStore interface.
// It links a source file into the datastore structure based on handler type.
func (s *filesystemDataStore) CreateDataLink(pack, handlerName, sourceFile string) (string, error) {
	baseName := filepath.Base(sourceFile)
	linkDir := s.paths.PackHandlerDir(pack, handlerName)
	linkPath := filepath.Join(linkDir, baseName)

	if err := s.fs.MkdirAll(linkDir, 0755); err != nil {
		return "", fmt.Errorf("failed to create directory for pack %s handler %s: %w", pack, handlerName, err)
	}

	// If the link already exists and points to the correct source, do nothing.
	if currentTarget, err := s.fs.Readlink(linkPath); err == nil && currentTarget == sourceFile {
		return linkPath, nil
	}

	// If it exists but is wrong, remove it first.
	if _, err := s.fs.Lstat(linkPath); err == nil {
		if err := s.fs.Remove(linkPath); err != nil {
			return "", fmt.Errorf("failed to remove existing incorrect link: %w", err)
		}
	}

	if err := s.fs.Symlink(sourceFile, linkPath); err != nil {
		return "", fmt.Errorf("failed to create symlink: %w", err)
	}

	return linkPath, nil
}

// CreateUserLink implements the simplified DataStore interface.
// It creates a user-visible symlink from datastore to user location.
func (s *filesystemDataStore) CreateUserLink(datastorePath, userPath string) error {
	// Expand home directory if needed
	expandedPath := userPath
	if strings.HasPrefix(userPath, "~/") {
		home, err := os.UserHomeDir()
		if err != nil {
			return fmt.Errorf("failed to get home directory: %w", err)
		}
		expandedPath = filepath.Join(home, userPath[2:])
	}

	// Ensure parent directory exists
	parentDir := filepath.Dir(expandedPath)
	if err := s.fs.MkdirAll(parentDir, 0755); err != nil {
		return fmt.Errorf("failed to create parent directory: %w", err)
	}

	// Remove existing file/link if present
	if err := s.fs.Remove(expandedPath); err != nil && !os.IsNotExist(err) {
		// Try to check if it's a directory
		if stat, statErr := s.fs.Stat(expandedPath); statErr == nil && stat.IsDir() {
			return fmt.Errorf("target path is a directory: %s", expandedPath)
		}
	}

	// Create the symlink
	if err := s.fs.Symlink(datastorePath, expandedPath); err != nil {
		return fmt.Errorf("failed to create symlink: %w", err)
	}

	return nil
}

// RunAndRecord implements the simplified DataStore interface.
// It executes a command and records completion with a sentinel.
func (s *filesystemDataStore) RunAndRecord(pack, handlerName, command, sentinel string) error {
	// Check if already run using the existing method
	sentinelDir := s.paths.PackHandlerDir(pack, handlerName)
	sentinelPath := filepath.Join(sentinelDir, sentinel)

	// Check if sentinel exists
	if _, err := s.fs.Stat(sentinelPath); err == nil {
		// Already run, skip
		return nil
	}

	// Execute the command
	cmd := exec.Command("sh", "-c", command)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("command failed: %w\nOutput: %s", err, output)
	}

	// Record completion
	if err := s.fs.MkdirAll(sentinelDir, 0755); err != nil {
		return fmt.Errorf("failed to create sentinel directory: %w", err)
	}

	// Write sentinel with timestamp
	content := fmt.Sprintf("completed|%s", time.Now().Format(time.RFC3339))
	if err := s.fs.WriteFile(sentinelPath, []byte(content), 0644); err != nil {
		return fmt.Errorf("failed to write sentinel file: %w", err)
	}

	return nil
}

// HasSentinel implements the simplified DataStore interface.
// It checks if an operation has been completed.
func (s *filesystemDataStore) HasSentinel(pack, handlerName, sentinel string) (bool, error) {
	sentinelPath := filepath.Join(s.paths.PackHandlerDir(pack, handlerName), sentinel)
	_, err := s.fs.Stat(sentinelPath)
	if err == nil {
		return true, nil
	}
	if os.IsNotExist(err) {
		return false, nil
	}
	return false, fmt.Errorf("failed to check sentinel: %w", err)
}

// RemoveState implements the simplified DataStore interface.
// It removes all state for a handler in a pack.
func (s *filesystemDataStore) RemoveState(pack, handlerName string) error {
	stateDir := s.paths.PackHandlerDir(pack, handlerName)

	// Check if directory exists
	if _, err := s.fs.Stat(stateDir); os.IsNotExist(err) {
		// Nothing to remove
		return nil
	}

	// Remove the entire state directory
	if err := s.fs.RemoveAll(stateDir); err != nil {
		return fmt.Errorf("failed to remove state directory: %w", err)
	}

	return nil
}
