package pack

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusOptions contains options for getting pack status
type StatusOptions struct {
	// Pack is the pack to get status for
	Pack types.Pack

	// DataStore is used to check deployment state
	DataStore types.DataStore

	// FileSystem to use for file operations
	FileSystem types.FS

	// Paths provides system paths
	Paths paths.Paths
}

// StatusResult contains the status information for a pack
type StatusResult struct {
	// Pack name
	Name string

	// Whether pack has .dodot.toml configuration
	HasConfig bool

	// Whether pack is ignored via .dodotignore
	IsIgnored bool

	// Overall pack status
	Status string

	// Status of individual files
	Files []FileStatus
}

// FileStatus contains status information for a single file
type FileStatus struct {
	// Handler that would process this file
	Handler string

	// Relative path within the pack
	Path string

	// Absolute path to the file
	AbsolutePath string

	// Current deployment status
	Status types.Status

	// Additional handler-specific information
	AdditionalInfo string
}

// GetStatus returns the deployment status for a pack
func GetStatus(opts StatusOptions) (*StatusResult, error) {
	logger := logging.GetLogger("pack.status").With().
		Str("pack", opts.Pack.Name).
		Logger()

	result := &StatusResult{
		Name:  opts.Pack.Name,
		Files: []FileStatus{},
	}

	// Check for special files (.dodot.toml, .dodotignore)
	if err := checkSpecialFiles(opts.Pack, result, opts.FileSystem); err != nil {
		return nil, err
	}

	// If pack is ignored, no need to process handlers
	if result.IsIgnored {
		result.Status = "ignored"
		return result, nil
	}

	// Get all rule matches for this pack
	matches, err := rules.GetMatchesFS([]types.Pack{opts.Pack}, opts.FileSystem)
	if err != nil {
		return nil, fmt.Errorf("failed to process rules: %w", err)
	}

	logger.Debug().
		Int("matchCount", len(matches)).
		Msg("Got rule matches for pack")

	// Process each match to get status
	for _, match := range matches {
		// Get handler status from datastore
		status, err := getHandlerStatus(match, opts.Pack, opts.DataStore, opts.FileSystem, opts.Paths)
		if err != nil {
			logger.Error().
				Err(err).
				Str("file", match.Path).
				Str("handler", match.HandlerName).
				Msg("Failed to get handler status")
			// Add error status for this file
			status = types.Status{
				State:   types.StatusStateError,
				Message: fmt.Sprintf("status check failed: %v", err),
			}
		}

		fileStatus := FileStatus{
			Handler:        match.HandlerName,
			Path:           match.Path,
			AbsolutePath:   match.AbsolutePath,
			Status:         status,
			AdditionalInfo: getHandlerAdditionalInfo(match.HandlerName, match.Path, opts.Pack, opts.Paths),
		}

		result.Files = append(result.Files, fileStatus)
	}

	// Calculate aggregated pack status
	result.Status = calculatePackStatus(result.Files)

	logger.Debug().
		Str("status", result.Status).
		Int("fileCount", len(result.Files)).
		Msg("Pack status determined")

	return result, nil
}

// checkSpecialFiles checks for .dodot.toml and .dodotignore files
func checkSpecialFiles(pack types.Pack, result *StatusResult, fs types.FS) error {
	// Check for .dodotignore
	ignorePath := filepath.Join(pack.Path, ".dodotignore")
	if _, err := fs.Stat(ignorePath); err == nil {
		result.IsIgnored = true
	}

	// Check for .dodot.toml
	configPath := filepath.Join(pack.Path, ".dodot.toml")
	if _, err := fs.Stat(configPath); err == nil {
		result.HasConfig = true
	} else if !os.IsNotExist(err) {
		return fmt.Errorf("failed to check config file: %w", err)
	}

	return nil
}

// getHandlerStatus checks the deployment status for a specific match
func getHandlerStatus(match types.RuleMatch, pack types.Pack, dataStore types.DataStore, fs types.FS, pathsInstance paths.Paths) (types.Status, error) {
	// Check handler category to determine how to check status
	category := handlers.HandlerRegistry.GetHandlerCategory(match.HandlerName)

	switch category {
	case handlers.CategoryConfiguration:
		// For configuration handlers (symlink, path, shell), check intermediate links
		return getConfigurationHandlerStatus(match, pack, dataStore, fs, pathsInstance)
	case handlers.CategoryCodeExecution:
		// For code execution handlers (install, homebrew), check sentinels
		return getCodeExecutionHandlerStatus(match, pack, dataStore, fs)
	default:
		return types.Status{
			State:   types.StatusStateUnknown,
			Message: "unknown handler type",
		}, nil
	}
}

// getConfigurationHandlerStatus checks status for configuration handlers
func getConfigurationHandlerStatus(match types.RuleMatch, pack types.Pack, dataStore types.DataStore, fs types.FS, pathsInstance paths.Paths) (types.Status, error) {
	baseName := filepath.Base(match.Path)
	intermediateLinkPath := filepath.Join(pathsInstance.PackHandlerDir(pack.Name, match.HandlerName), baseName)

	// Check if intermediate link exists and is valid
	exists, valid, err := checkIntermediateLink(fs, intermediateLinkPath, match.AbsolutePath)
	if err != nil {
		return types.Status{}, err
	}

	if !exists {
		message := "not deployed"
		switch match.HandlerName {
		case "symlink":
			message = "not linked"
		case "path":
			message = "not in PATH"
		case "shell":
			message = "not sourced in shell"
		}
		return types.Status{
			State:   types.StatusStateMissing,
			Message: message,
		}, nil
	}

	if !valid {
		message := "link points to wrong source"
		switch match.HandlerName {
		case "path":
			message = "PATH link points to wrong directory"
		case "shell":
			message = "shell profile link points to wrong script"
		}
		return types.Status{
			State:   types.StatusStateError,
			Message: message,
			ErrorDetails: &types.StatusErrorDetails{
				ErrorType:        "invalid_intermediate",
				IntermediatePath: intermediateLinkPath,
				SourcePath:       match.AbsolutePath,
			},
		}, nil
	}

	// Check if source file still exists (for symlinks)
	if match.HandlerName == "symlink" {
		if _, err := fs.Stat(match.AbsolutePath); err != nil {
			if os.IsNotExist(err) {
				return types.Status{
					State:   types.StatusStateError,
					Message: "source file missing",
					ErrorDetails: &types.StatusErrorDetails{
						ErrorType:        "missing_source",
						IntermediatePath: intermediateLinkPath,
						SourcePath:       match.AbsolutePath,
					},
				}, nil
			}
		}
	}

	message := "deployed"
	switch match.HandlerName {
	case "symlink":
		message = "linked"
	case "path":
		message = "added to PATH"
	case "shell":
		message = "sourced in shell profile"
	}

	return types.Status{
		State:   types.StatusStateReady,
		Message: message,
	}, nil
}

// getCodeExecutionHandlerStatus checks status for code execution handlers
func getCodeExecutionHandlerStatus(match types.RuleMatch, pack types.Pack, dataStore types.DataStore, fs types.FS) (types.Status, error) {
	// Calculate current checksum
	currentChecksum, err := hashutil.CalculateFileChecksum(match.AbsolutePath)
	if err != nil {
		return types.Status{}, fmt.Errorf("failed to calculate checksum: %w", err)
	}

	// Determine sentinel name based on handler type
	var sentinelName string
	switch match.HandlerName {
	case "install":
		sentinelName = fmt.Sprintf("%s-%s", filepath.Base(match.Path), currentChecksum)
	case "homebrew":
		sentinelName = fmt.Sprintf("%s_%s-%s", pack.Name, filepath.Base(match.Path), currentChecksum)
	default:
		sentinelName = fmt.Sprintf("%s.sentinel", match.Path)
	}

	// Check if sentinel exists
	hasSentinel, err := dataStore.HasSentinel(pack.Name, match.HandlerName, sentinelName)
	if err != nil {
		return types.Status{}, err
	}

	if !hasSentinel {
		// Check if there's an old sentinel with different checksum
		sentinels, err := dataStore.ListHandlerSentinels(pack.Name, match.HandlerName)
		if err != nil {
			return types.Status{}, err
		}

		// Look for any sentinel for this file
		fileBaseName := filepath.Base(match.Path)
		var oldSentinelFound bool
		for _, s := range sentinels {
			if strings.Contains(s, fileBaseName) && s != sentinelName {
				oldSentinelFound = true
				break
			}
		}

		if oldSentinelFound {
			return types.Status{
				State:   types.StatusStatePending,
				Message: "file changed, needs re-run",
			}, nil
		}

		message := "never run"
		if match.HandlerName == "homebrew" {
			message = "never installed"
		}
		return types.Status{
			State:   types.StatusStateMissing,
			Message: message,
		}, nil
	}

	message := "provisioned"
	if match.HandlerName == "homebrew" {
		message = "packages installed"
	}

	return types.Status{
		State:     types.StatusStateReady,
		Message:   message,
		Timestamp: nil,
	}, nil
}

// checkIntermediateLink checks if an intermediate link exists and is valid
func checkIntermediateLink(fs types.FS, intermediateLinkPath, expectedTarget string) (exists bool, valid bool, err error) {
	info, err := fs.Lstat(intermediateLinkPath)
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

	target, err := fs.Readlink(intermediateLinkPath)
	if err != nil {
		return true, false, fmt.Errorf("failed to read link target: %w", err)
	}

	return true, target == expectedTarget, nil
}

// getHandlerAdditionalInfo returns additional display information based on handler type
func getHandlerAdditionalInfo(handlerName string, filePath string, pack types.Pack, pathsInstance paths.Paths) string {
	switch handlerName {
	case "symlink":
		// For symlinks, show the target path
		targetPath := pathsInstance.MapPackFileToSystem(&pack, filePath)
		homeDir := os.Getenv("HOME")
		return types.FormatSymlinkForDisplay(targetPath, homeDir, 46)
	case "path":
		return "add to $PATH"
	case "shell":
		// Detect shell type from filename
		fileName := filepath.Base(filePath)
		if strings.Contains(fileName, "bash") {
			return "bash profile"
		} else if strings.Contains(fileName, "zsh") {
			return "zsh profile"
		} else if strings.Contains(fileName, "fish") {
			return "fish config"
		}
		return "shell source"
	case "install":
		return "run script"
	case "homebrew":
		return "brew install"
	default:
		return ""
	}
}

// calculatePackStatus calculates the overall pack status based on file statuses
func calculatePackStatus(files []FileStatus) string {
	if len(files) == 0 {
		return "queue"
	}

	hasError := false
	hasWarning := false
	allSuccess := true

	for _, file := range files {
		// Map internal states to display statuses
		displayStatus := statusStateToDisplayStatus(file.Status.State)

		// Skip config files in status calculation
		if displayStatus == "config" {
			continue
		}

		if displayStatus == "error" {
			hasError = true
		}
		if displayStatus == "warning" {
			hasWarning = true
		}
		if displayStatus != "success" {
			allSuccess = false
		}
	}

	if hasError {
		return "alert" // Will be displayed with ALERT styling
	}
	if hasWarning {
		return "partial" // Has warnings but no errors
	}
	if allSuccess {
		return "success"
	}
	return "queue"
}

// statusStateToDisplayStatus converts internal status states to display status strings
func statusStateToDisplayStatus(state types.StatusState) string {
	switch state {
	case types.StatusStateReady, types.StatusStateSuccess:
		return "success"
	case types.StatusStateMissing:
		return "queue"
	case types.StatusStatePending:
		return "queue"
	case types.StatusStateError:
		return "error"
	case types.StatusStateIgnored:
		return "ignored"
	case types.StatusStateConfig:
		return "config"
	default:
		return "unknown"
	}
}
