package datastore

import (
	"fmt"
	"os"
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

func (s *filesystemDataStore) Link(pack, sourceFile string) (string, error) {
	baseName := filepath.Base(sourceFile)
	intermediateLinkDir := s.paths.PackHandlerDir(pack, "symlinks")
	intermediateLinkPath := filepath.Join(intermediateLinkDir, baseName)

	if err := s.fs.MkdirAll(intermediateLinkDir, 0755); err != nil {
		return "", fmt.Errorf("failed to create intermediate directory for pack %s: %w", pack, err)
	}

	// If the link already exists and points to the correct source, do nothing.
	if currentTarget, err := s.fs.Readlink(intermediateLinkPath); err == nil && currentTarget == sourceFile {
		return intermediateLinkPath, nil
	}

	// If it exists but is wrong, remove it first.
	if _, err := s.fs.Lstat(intermediateLinkPath); err == nil {
		if err := s.fs.Remove(intermediateLinkPath); err != nil {
			return "", fmt.Errorf("failed to remove existing incorrect intermediate link: %w", err)
		}
	}

	if err := s.fs.Symlink(sourceFile, intermediateLinkPath); err != nil {
		return "", fmt.Errorf("failed to create intermediate symlink: %w", err)
	}

	return intermediateLinkPath, nil
}

func (s *filesystemDataStore) Unlink(pack, sourceFile string) error {
	baseName := filepath.Base(sourceFile)
	intermediateLinkPath := filepath.Join(s.paths.PackHandlerDir(pack, "symlinks"), baseName)

	// If the link doesn't exist, there's nothing to do.
	if _, err := s.fs.Lstat(intermediateLinkPath); err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("failed to stat intermediate link: %w", err)
	}

	if err := s.fs.Remove(intermediateLinkPath); err != nil {
		return fmt.Errorf("failed to remove intermediate symlink: %w", err)
	}

	return nil
}

func (s *filesystemDataStore) AddToPath(pack, dirPath string) error {
	baseName := filepath.Base(dirPath)
	intermediateLinkDir := s.paths.PackHandlerDir(pack, "path")
	intermediateLinkPath := filepath.Join(intermediateLinkDir, baseName)

	if err := s.fs.MkdirAll(intermediateLinkDir, 0755); err != nil {
		return fmt.Errorf("failed to create path directory for pack %s: %w", pack, err)
	}

	// If the link already exists and points to the correct source, do nothing.
	if currentTarget, err := s.fs.Readlink(intermediateLinkPath); err == nil && currentTarget == dirPath {
		return nil
	}

	// If it exists but is wrong, remove it first.
	if _, err := s.fs.Lstat(intermediateLinkPath); err == nil {
		if err := s.fs.Remove(intermediateLinkPath); err != nil {
			return fmt.Errorf("failed to remove existing incorrect path link: %w", err)
		}
	}

	if err := s.fs.Symlink(dirPath, intermediateLinkPath); err != nil {
		return fmt.Errorf("failed to create path symlink: %w", err)
	}

	return nil
}

func (s *filesystemDataStore) AddToShellProfile(pack, scriptPath string) error {
	baseName := filepath.Base(scriptPath)
	intermediateLinkDir := s.paths.PackHandlerDir(pack, "shell_profile")
	intermediateLinkPath := filepath.Join(intermediateLinkDir, baseName)

	if err := s.fs.MkdirAll(intermediateLinkDir, 0755); err != nil {
		return fmt.Errorf("failed to create shell_profile directory for pack %s: %w", pack, err)
	}

	// If the link already exists and points to the correct source, do nothing.
	if currentTarget, err := s.fs.Readlink(intermediateLinkPath); err == nil && currentTarget == scriptPath {
		return nil
	}

	// If it exists but is wrong, remove it first.
	if _, err := s.fs.Lstat(intermediateLinkPath); err == nil {
		if err := s.fs.Remove(intermediateLinkPath); err != nil {
			return fmt.Errorf("failed to remove existing incorrect shell_profile link: %w", err)
		}
	}

	if err := s.fs.Symlink(scriptPath, intermediateLinkPath); err != nil {
		return fmt.Errorf("failed to create shell_profile symlink: %w", err)
	}

	return nil
}

func (s *filesystemDataStore) RecordProvisioning(pack, sentinelName, checksum string) error {
	sentinelDir := s.paths.PackHandlerDir(pack, "sentinels")
	sentinelPath := filepath.Join(sentinelDir, sentinelName)

	if err := s.fs.MkdirAll(sentinelDir, 0755); err != nil {
		return fmt.Errorf("failed to create sentinels directory for pack %s: %w", pack, err)
	}

	// Use pipe separator to avoid conflicts with checksums that contain colons
	content := fmt.Sprintf("%s|%s", checksum, time.Now().Format(time.RFC3339))
	if err := s.fs.WriteFile(sentinelPath, []byte(content), 0644); err != nil {
		return fmt.Errorf("failed to write sentinel file: %w", err)
	}

	return nil
}

func (s *filesystemDataStore) NeedsProvisioning(pack, sentinelName, checksum string) (bool, error) {
	sentinelPath := filepath.Join(s.paths.PackHandlerDir(pack, "sentinels"), sentinelName)

	content, err := s.fs.ReadFile(sentinelPath)
	if err != nil {
		if os.IsNotExist(err) {
			return true, nil
		}
		return false, fmt.Errorf("failed to read sentinel file: %w", err)
	}

	// The content format is "checksum|timestamp"
	parts := strings.SplitN(string(content), "|", 2)
	if len(parts) < 1 {
		// Invalid sentinel file, assume provisioning is needed
		return true, nil
	}

	return parts[0] != checksum, nil
}

func (s *filesystemDataStore) GetStatus(pack, sourceFile string) (types.Status, error) {
	baseName := filepath.Base(sourceFile)
	intermediateLinkPath := filepath.Join(s.paths.PackHandlerDir(pack, "symlinks"), baseName)

	_, err := s.fs.Lstat(intermediateLinkPath)
	if err != nil {
		if os.IsNotExist(err) {
			return types.Status{State: types.StatusStateMissing, Message: "link not deployed"}, nil
		}
		return types.Status{}, fmt.Errorf("failed to stat intermediate link: %w", err)
	}

	// TODO: Add more detailed status checks (e.g., dangling links)
	return types.Status{State: types.StatusStateReady, Message: "linked"}, nil
}
