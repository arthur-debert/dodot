// Package status provides the status command implementation for dodot.
//
// The status command shows the deployment state of packs and files,
// answering two key questions:
//   - What has already been deployed? (current state)
//   - What will happen if I deploy? (predicted state)
package status

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusPacksOptions contains options for the status command
type StatusPacksOptions struct {
	// DotfilesRoot is the root directory containing packs
	DotfilesRoot string

	// PackNames specifies which packs to check status for
	// If empty, all packs are checked
	PackNames []string

	// Paths provides system paths (required)
	Paths types.Pather

	// FileSystem to use (defaults to OS filesystem)
	FileSystem types.FS
}

// StatusPacks shows the deployment status of specified packs
func StatusPacks(opts StatusPacksOptions) (*types.DisplayResult, error) {
	logger := logging.GetLogger("commands.status")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Msg("Starting status command")

	// Initialize filesystem if not provided
	if opts.FileSystem == nil {
		opts.FileSystem = filesystem.NewOS()
	}

	// Initialize paths if not provided
	if opts.Paths == nil {
		p, err := paths.New(opts.DotfilesRoot)
		if err != nil {
			return nil, fmt.Errorf("failed to initialize paths: %w", err)
		}
		opts.Paths = p
	}

	// Use centralized pack discovery and selection with filesystem support
	selectedPacks, err := core.DiscoverAndSelectPacksFS(opts.DotfilesRoot, opts.PackNames, opts.FileSystem)
	if err != nil {
		return nil, err
	}

	logger.Info().
		Int("packCount", len(selectedPacks)).
		Msg("Found packs to check")

	// Create datastore for status checking
	dataStore := datastore.New(opts.FileSystem, opts.Paths.(paths.Paths))

	// Build display result
	result := &types.DisplayResult{
		Command:   "status",
		DryRun:    false,
		Timestamp: time.Now(),
		Packs:     make([]types.DisplayPack, 0, len(selectedPacks)),
	}

	// Process each pack
	for _, pack := range selectedPacks {
		displayPack, err := getPackDisplayStatus(pack, dataStore, opts.FileSystem, opts.Paths.(paths.Paths))
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to get pack status")
			// Continue with other packs even if one fails
			continue
		}
		result.Packs = append(result.Packs, *displayPack)
	}

	return result, nil
}

// getPackDisplayStatus generates display status for a single pack
func getPackDisplayStatus(pack types.Pack, dataStore types.DataStore, fs types.FS, pathsInstance paths.Paths) (*types.DisplayPack, error) {
	logger := logging.GetLogger("commands.status").With().
		Str("pack", pack.Name).
		Logger()

	displayPack := &types.DisplayPack{
		Name:      pack.Name,
		Files:     []types.DisplayFile{},
		HasConfig: false,
		IsIgnored: false,
	}

	// Check for special files (.dodot.toml, .dodotignore)
	if err := checkSpecialFiles(pack, displayPack, fs); err != nil {
		return nil, err
	}

	// If pack is ignored, no need to process handlers
	if displayPack.IsIgnored {
		displayPack.Status = "ignored"
		return displayPack, nil
	}

	// Get all rule matches for this pack
	matches, err := rules.GetMatchesFS([]types.Pack{pack}, fs)
	if err != nil {
		return nil, fmt.Errorf("failed to process rules: %w", err)
	}

	logger.Debug().
		Int("matchCount", len(matches)).
		Msg("Got rule matches for pack")

	// Process each match to get status
	for _, match := range matches {
		// Get handler status from datastore
		status, err := getHandlerStatus(match, pack, dataStore, fs, pathsInstance)
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

		// Convert to display format
		displayFile := types.DisplayFile{
			Handler:        match.HandlerName,
			Path:           match.Path,
			Status:         statusStateToDisplayStatus(status.State),
			Message:        status.Message,
			LastExecuted:   status.Timestamp,
			HandlerSymbol:  types.GetHandlerSymbol(match.HandlerName),
			AdditionalInfo: getHandlerAdditionalInfo(match.HandlerName, match.Path, match.AbsolutePath, pathsInstance),
		}

		displayPack.Files = append(displayPack.Files, displayFile)
	}

	// Calculate aggregated pack status
	displayPack.Status = displayPack.GetPackStatus()

	logger.Debug().
		Str("status", displayPack.Status).
		Int("fileCount", len(displayPack.Files)).
		Msg("Pack status determined")

	return displayPack, nil
}

// checkSpecialFiles checks for .dodot.toml and .dodotignore files
func checkSpecialFiles(pack types.Pack, displayPack *types.DisplayPack, fs types.FS) error {
	// Check for .dodotignore
	ignorePath := filepath.Join(pack.Path, ".dodotignore")
	if _, err := fs.Stat(ignorePath); err == nil {
		displayPack.IsIgnored = true
		displayPack.Files = append(displayPack.Files, types.DisplayFile{
			Path:   ".dodotignore",
			Status: "ignored",
		})
	}

	// Check for .dodot.toml
	configPath := filepath.Join(pack.Path, ".dodot.toml")
	if _, err := fs.Stat(configPath); err == nil {
		displayPack.HasConfig = true
		displayPack.Files = append(displayPack.Files, types.DisplayFile{
			Path:   ".dodot.toml",
			Status: "config",
		})
	} else if !os.IsNotExist(err) {
		return fmt.Errorf("failed to check config file: %w", err)
	}

	return nil
}

// getHandlerStatus checks the deployment status for a specific match using the new datastore
// This implements the semantic logic that was previously in the datastore
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

// getConfigurationHandlerStatus checks status for configuration handlers (symlink, path, shell)
// These handlers create intermediate links that we can check
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

// getCodeExecutionHandlerStatus checks status for code execution handlers (install, homebrew)
// These handlers use sentinels to track completion
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
		State:   types.StatusStateReady,
		Message: message,
		// Note: We don't have timestamp in the new system
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
func getHandlerAdditionalInfo(handlerName string, filePath string, absolutePath string, pathsInstance paths.Paths) string {
	switch handlerName {
	case "symlink":
		// For symlinks, show the target path
		// Use the paths instance to determine where symlink would go
		pack := &types.Pack{} // We'd need the actual pack here for proper mapping
		targetPath := pathsInstance.MapPackFileToSystem(pack, filePath)
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
