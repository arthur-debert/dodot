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

// getPreviousRun retrieves information about the last execution of a handler
// Returns the timestamp and checksum if available, or nil if never run
func (s *filesystemDataStore) getPreviousRun(pack, handler, sentinelName string) (*time.Time, string, error) {
	sentinelPath := filepath.Join(s.paths.PackHandlerDir(pack, "sentinels"), sentinelName)

	content, err := s.fs.ReadFile(sentinelPath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, "", nil
		}
		return nil, "", fmt.Errorf("failed to read sentinel file: %w", err)
	}

	// The content format is "checksum|timestamp"
	parts := strings.SplitN(string(content), "|", 2)
	if len(parts) < 2 {
		// Invalid sentinel file format
		return nil, "", nil
	}

	timestamp, err := time.Parse(time.RFC3339, parts[1])
	if err != nil {
		// Invalid timestamp, ignore it
		return nil, parts[0], nil
	}

	return &timestamp, parts[0], nil
}

// checkIntermediateLink checks if an intermediate link exists and is valid
func (s *filesystemDataStore) checkIntermediateLink(intermediateLinkPath, expectedTarget string) (exists bool, valid bool, err error) {
	info, err := s.fs.Lstat(intermediateLinkPath)
	if err != nil {
		if os.IsNotExist(err) {
			return false, false, nil
		}
		return false, false, fmt.Errorf("failed to stat intermediate link: %w", err)
	}

	if info.Mode()&os.ModeSymlink == 0 {
		// File exists but is not a symlink
		return true, false, nil
	}

	target, err := s.fs.Readlink(intermediateLinkPath)
	if err != nil {
		return true, false, fmt.Errorf("failed to read link target: %w", err)
	}

	return true, target == expectedTarget, nil
}

// GetStatus checks the deployment status of any handler type
func (s *filesystemDataStore) GetStatus(pack, sourceFile string) (types.Status, error) {
	// This is the general entry point, but we need more context to determine handler type
	// For now, default to symlink handler behavior for backward compatibility
	return s.GetSymlinkStatus(pack, sourceFile)
}

// GetSymlinkStatus checks the status of a symlink deployment
func (s *filesystemDataStore) GetSymlinkStatus(pack, sourceFile string) (types.Status, error) {
	baseName := filepath.Base(sourceFile)
	intermediateLinkPath := filepath.Join(s.paths.PackHandlerDir(pack, "symlinks"), baseName)

	exists, valid, err := s.checkIntermediateLink(intermediateLinkPath, sourceFile)
	if err != nil {
		return types.Status{}, err
	}

	if !exists {
		return types.Status{
			State:   types.StatusStateMissing,
			Message: "not linked",
		}, nil
	}

	if !valid {
		return types.Status{
			State:   types.StatusStateError,
			Message: "intermediate link points to wrong source",
			ErrorDetails: &types.StatusErrorDetails{
				ErrorType:        "invalid_intermediate",
				IntermediatePath: intermediateLinkPath,
				SourcePath:       sourceFile,
			},
		}, nil
	}

	// Check if source file still exists
	// Note: We check using the absolute path since the filesystem might be relative
	if _, err := s.fs.Stat(sourceFile); err != nil {
		if os.IsNotExist(err) {
			return types.Status{
				State:   types.StatusStateError,
				Message: "source file missing",
				ErrorDetails: &types.StatusErrorDetails{
					ErrorType:        "missing_source",
					IntermediatePath: intermediateLinkPath,
					SourcePath:       sourceFile,
				},
			}, nil
		}
		// If it's not "not exist", it might be because we're using relative paths
		// Return ready status if we can't determine file existence
	}

	return types.Status{
		State:   types.StatusStateReady,
		Message: "linked",
	}, nil
}

// GetPathStatus checks the status of a PATH directory deployment
func (s *filesystemDataStore) GetPathStatus(pack, dirPath string) (types.Status, error) {
	baseName := filepath.Base(dirPath)
	intermediateLinkPath := filepath.Join(s.paths.PackHandlerDir(pack, "path"), baseName)

	exists, valid, err := s.checkIntermediateLink(intermediateLinkPath, dirPath)
	if err != nil {
		return types.Status{}, err
	}

	if !exists {
		return types.Status{
			State:   types.StatusStateMissing,
			Message: "not in PATH",
		}, nil
	}

	if !valid {
		return types.Status{
			State:   types.StatusStateError,
			Message: "PATH link points to wrong directory",
		}, nil
	}

	return types.Status{
		State:   types.StatusStateReady,
		Message: "added to PATH",
	}, nil
}

// GetShellProfileStatus checks the status of a shell profile script deployment
func (s *filesystemDataStore) GetShellProfileStatus(pack, scriptPath string) (types.Status, error) {
	baseName := filepath.Base(scriptPath)
	intermediateLinkPath := filepath.Join(s.paths.PackHandlerDir(pack, "shell_profile"), baseName)

	exists, valid, err := s.checkIntermediateLink(intermediateLinkPath, scriptPath)
	if err != nil {
		return types.Status{}, err
	}

	if !exists {
		return types.Status{
			State:   types.StatusStateMissing,
			Message: "not sourced in shell",
		}, nil
	}

	if !valid {
		return types.Status{
			State:   types.StatusStateError,
			Message: "shell profile link points to wrong script",
		}, nil
	}

	return types.Status{
		State:   types.StatusStateReady,
		Message: "sourced in shell profile",
	}, nil
}

// GetProvisioningStatus checks the status of a provisioning action
func (s *filesystemDataStore) GetProvisioningStatus(pack, sentinelName, currentChecksum string) (types.Status, error) {
	timestamp, lastChecksum, err := s.getPreviousRun(pack, "provision", sentinelName)
	if err != nil {
		return types.Status{}, err
	}

	if timestamp == nil {
		return types.Status{
			State:   types.StatusStateMissing,
			Message: "never run",
		}, nil
	}

	if lastChecksum != currentChecksum {
		return types.Status{
			State:     types.StatusStatePending,
			Message:   "file changed, needs re-run",
			Timestamp: timestamp,
		}, nil
	}

	return types.Status{
		State:     types.StatusStateReady,
		Message:   "provisioned",
		Timestamp: timestamp,
	}, nil
}

// GetBrewStatus checks the status of a Homebrew deployment
func (s *filesystemDataStore) GetBrewStatus(pack, brewfilePath, currentChecksum string) (types.Status, error) {
	sentinelName := fmt.Sprintf("homebrew-%s.sentinel", pack)
	timestamp, lastChecksum, err := s.getPreviousRun(pack, "homebrew", sentinelName)
	if err != nil {
		return types.Status{}, err
	}

	if timestamp == nil {
		return types.Status{
			State:   types.StatusStateMissing,
			Message: "never installed",
		}, nil
	}

	if lastChecksum != currentChecksum {
		return types.Status{
			State:     types.StatusStatePending,
			Message:   "Brewfile changed, needs update",
			Timestamp: timestamp,
		}, nil
	}

	return types.Status{
		State:     types.StatusStateReady,
		Message:   "packages installed",
		Timestamp: timestamp,
	}, nil
}
