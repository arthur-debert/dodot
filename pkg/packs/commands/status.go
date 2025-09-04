package commands

import (
	"fmt"
	"path/filepath"
	"time"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/install"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/path"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/shell"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/symlink"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/packs/discovery"
	"github.com/arthur-debert/dodot/pkg/packs/orchestration"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/display"
)

// StatusCommand implements the "status" command using the pack orchestration.
type StatusCommand struct{}

// Name returns the command name.
func (c *StatusCommand) Name() string {
	return "status"
}

// StatusOptions contains options for getting pack status
type StatusOptions struct {
	Pack       types.Pack
	DataStore  datastore.DataStore
	FileSystem types.FS
	Paths      paths.Paths
}

// StatusState represents the state of a deployment
type StatusState string

const (
	// StatusStatePending indicates the action has not been executed yet
	StatusStatePending StatusState = "pending"
	// StatusStateReady indicates the action was executed and is ready
	StatusStateReady StatusState = "ready"
	// StatusStateSuccess indicates success
	StatusStateSuccess StatusState = "success"
	// StatusStateMissing indicates missing files
	StatusStateMissing StatusState = "missing"
	// StatusStateError indicates an error occurred
	StatusStateError StatusState = "error"
	// StatusStateIgnored indicates the file/pack is ignored
	StatusStateIgnored StatusState = "ignored"
	// StatusStateConfig indicates this is a config file
	StatusStateConfig StatusState = "config"
)

// Status represents the status of a single item
type Status struct {
	State     StatusState
	Message   string
	Timestamp *time.Time
}

// FileStatus represents the status of a single file
type FileStatus struct {
	Handler        string
	Path           string
	Status         Status
	AdditionalInfo string
}

// StatusResult represents the complete status of a pack
type StatusResult struct {
	Name      string
	Path      string
	HasConfig bool
	IsIgnored bool
	Status    string
	Files     []FileStatus
}

// ExecuteForPack executes the "status" command for a single pack.
func (c *StatusCommand) ExecuteForPack(pack types.Pack, opts orchestration.Options) (*orchestration.PackResult, error) {
	logger := logging.GetLogger("orchestration.status")
	logger.Debug().
		Str("pack", pack.Name).
		Msg("Executing status command for pack")

	// Initialize filesystem
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Initialize paths if not provided
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return &orchestration.PackResult{
			Pack:    pack,
			Success: false,
			Error:   err,
		}, err
	}

	// Create datastore for status checking
	dataStore := datastore.New(fs, pathsInstance)

	// Get pack status
	statusOpts := StatusOptions{
		Pack:       pack,
		DataStore:  dataStore,
		FileSystem: fs,
		Paths:      pathsInstance,
	}

	packStatus, err := GetStatus(statusOpts)
	if err != nil {
		logger.Error().
			Err(err).
			Str("pack", pack.Name).
			Msg("Failed to get pack status")
		return &orchestration.PackResult{
			Pack:    pack,
			Success: false,
			Error:   err,
		}, err
	}

	logger.Info().
		Str("pack", pack.Name).
		Str("status", packStatus.Status).
		Int("fileCount", len(packStatus.Files)).
		Msg("Status command completed for pack")

	return &orchestration.PackResult{
		Pack:                  pack,
		Success:               true,
		Error:                 nil,
		CommandSpecificResult: packStatus,
	}, nil
}

// GetStatus retrieves the status of a pack
func GetStatus(opts StatusOptions) (*StatusResult, error) {
	logger := logging.GetLogger("commands.status")
	logger.Debug().
		Str("pack", opts.Pack.Name).
		Msg("Getting pack status")

	// Check for special files
	configPath := filepath.Join(opts.Pack.Path, ".dodot.toml")
	ignorePath := filepath.Join(opts.Pack.Path, ".dodotignore")

	_, hasConfig := opts.FileSystem.Stat(configPath)
	_, hasIgnore := opts.FileSystem.Stat(ignorePath)

	result := &StatusResult{
		Name:      opts.Pack.Name,
		Path:      opts.Pack.Path,
		HasConfig: hasConfig == nil,
		IsIgnored: hasIgnore == nil,
		Status:    "unknown",
		Files:     []FileStatus{},
	}

	// If pack is ignored, return early
	if result.IsIgnored {
		result.Status = "ignored"
		return result, nil
	}

	// Get matches for this pack
	matches, err := rules.NewMatcher().GetMatchesFS([]types.Pack{opts.Pack}, opts.FileSystem)
	if err != nil {
		return nil, fmt.Errorf("failed to get matches: %w", err)
	}

	// Check status for each match
	for _, match := range matches {
		fileStatus, err := getHandlerStatus(match, opts.Pack, opts.DataStore, opts.FileSystem, opts.Paths)
		if err != nil {
			logger.Error().
				Err(err).
				Str("file", match.Path).
				Str("handler", match.HandlerName).
				Msg("Failed to get handler status")
			continue
		}

		result.Files = append(result.Files, FileStatus{
			Handler:        match.HandlerName,
			Path:           match.Path,
			Status:         fileStatus,
			AdditionalInfo: "",
		})
	}

	// Determine overall pack status based on file statuses
	result.Status = determinePackStatus(result.Files)

	return result, nil
}

// getHandlerStatus checks the deployment status for a specific match
func getHandlerStatus(match rules.RuleMatch, pack types.Pack, dataStore datastore.DataStore, fs types.FS, pathsInstance paths.Paths) (Status, error) {
	logger := logging.GetLogger("commands.status.handler")

	// Create handler instance
	handler, err := createHandler(match.HandlerName)
	if err != nil {
		logger.Error().
			Err(err).
			Str("handler", match.HandlerName).
			Msg("Failed to create handler")
		return Status{
			State:   StatusStateError,
			Message: "Unknown handler",
		}, err
	}

	// Create status checker
	statusChecker := operations.NewDataStoreStatusChecker(dataStore, fs, pathsInstance)

	// Prepare file input
	fileInput := operations.FileInput{
		PackName:     pack.Name,
		SourcePath:   match.AbsolutePath,
		RelativePath: match.Path,
		Options:      match.HandlerOptions,
	}

	// Delegate status checking to handler
	handlerStatus, err := handler.CheckStatus(fileInput, statusChecker)
	if err != nil {
		logger.Error().
			Err(err).
			Str("file", match.Path).
			Str("handler", match.HandlerName).
			Msg("Handler status check failed")
		return Status{
			State:   StatusStateError,
			Message: handlerStatus.Message,
		}, err
	}

	// Convert handler status to command status
	return Status{
		State:   mapHandlerStatusToCommandStatus(handlerStatus.State),
		Message: handlerStatus.Message,
	}, nil
}

// mapHandlerStatusToCommandStatus converts handler status to command status
func mapHandlerStatusToCommandStatus(handlerState operations.StatusState) StatusState {
	switch handlerState {
	case operations.StatusStatePending:
		return StatusStatePending
	case operations.StatusStateReady:
		return StatusStateReady
	case operations.StatusStateError:
		return StatusStateError
	case operations.StatusStateUnknown:
		return StatusStateError
	default:
		return StatusStateError
	}
}

// createHandler creates a handler instance by name
func createHandler(name string) (operations.Handler, error) {
	// Import the handler creation logic from integration.go
	switch name {
	case "symlink":
		return symlink.NewHandler(), nil
	case "shell":
		return shell.NewHandler(), nil
	case "homebrew":
		return homebrew.NewHandler(), nil
	case "install":
		return install.NewHandler(), nil
	case "path":
		return path.NewHandler(), nil
	default:
		return nil, fmt.Errorf("unknown handler: %s", name)
	}
}

// determinePackStatus calculates the overall pack status from file statuses
func determinePackStatus(files []FileStatus) string {
	if len(files) == 0 {
		return "queue"
	}

	hasError := false
	hasSuccess := false
	hasPending := false

	for _, file := range files {
		switch file.Status.State {
		case StatusStateError:
			hasError = true
		case StatusStateReady, StatusStateSuccess:
			hasSuccess = true
		case StatusStatePending, StatusStateMissing:
			hasPending = true
		}
	}

	if hasError {
		return "error"
	}
	if hasPending && !hasSuccess {
		return "queue"
	}
	if hasPending && hasSuccess {
		return "partial"
	}
	if hasSuccess {
		return "success"
	}

	return "unknown"
}

// StatusCommandOptions contains options for the status command
type StatusCommandOptions struct {
	// DotfilesRoot is the root directory containing packs
	DotfilesRoot string

	// PackNames specifies which packs to check status for
	// If empty, all packs are checked
	PackNames []string

	// Paths provides system paths (optional, will be created if not provided)
	Paths types.Pather

	// FileSystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
}

// GetPacksStatus shows the deployment status of specified packs
// This is a query operation that uses core pack discovery but doesn't execute handlers.
func GetPacksStatus(opts StatusCommandOptions) (*display.PackCommandResult, error) {
	logger := logging.GetLogger("pack.status")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Msg("Starting status command")

	// Track any errors encountered
	var errors []error

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

	// Use core pack discovery (consistent with on/off commands)
	selectedPacks, err := discovery.DiscoverAndSelectPacksFS(opts.DotfilesRoot, opts.PackNames, opts.FileSystem)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to discover packs")
		return nil, err
	}

	logger.Info().
		Int("packCount", len(selectedPacks)).
		Msg("Found packs to check")

	// Create datastore for status checking
	dataStore := datastore.New(opts.FileSystem, opts.Paths.(paths.Paths))

	// Build command result
	result := &display.PackCommandResult{
		Command:   "status",
		DryRun:    false, // Status is always a query, never a dry run
		Timestamp: time.Now(),
		Packs:     make([]display.DisplayPack, 0, len(selectedPacks)),
		// Status command doesn't have a message
		Message: "",
	}

	// Process each pack using centralized status logic
	for _, p := range selectedPacks {
		// Get pack status using the centralized GetStatus function
		statusOpts := StatusOptions{
			Pack:       p,
			DataStore:  dataStore,
			FileSystem: opts.FileSystem,
			Paths:      opts.Paths.(paths.Paths),
		}

		packStatus, err := GetStatus(statusOpts)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", p.Name).
				Msg("Failed to get pack status")
			errors = append(errors, fmt.Errorf("pack %s: status check failed: %w", p.Name, err))
			// Continue with other packs even if one fails
			continue
		}

		// Convert to display format using existing conversion logic
		displayPack := convertStatusToDisplayPack(packStatus)
		result.Packs = append(result.Packs, displayPack)
	}

	logger.Info().
		Int("packsProcessed", len(result.Packs)).
		Int("errors", len(errors)).
		Msg("Status command completed")

	// Return error if any packs failed (but still return partial results)
	if len(errors) > 0 {
		return result, fmt.Errorf("status command encountered %d errors", len(errors))
	}

	return result, nil
}

// convertStatusToDisplayPack converts StatusResult to display.DisplayPack
func convertStatusToDisplayPack(status *StatusResult) display.DisplayPack {
	displayPack := display.DisplayPack{
		Name:      status.Name,
		HasConfig: status.HasConfig,
		IsIgnored: status.IsIgnored,
		Status:    status.Status,
		Files:     make([]display.DisplayFile, 0, len(status.Files)),
	}

	// Convert each file status
	for _, file := range status.Files {
		displayFile := display.DisplayFile{
			Handler:        file.Handler,
			Path:           file.Path,
			Status:         statusStateToDisplayStatus(file.Status.State),
			Message:        file.Status.Message,
			LastExecuted:   file.Status.Timestamp,
			HandlerSymbol:  display.GetHandlerSymbol(file.Handler),
			AdditionalInfo: file.AdditionalInfo,
		}
		displayPack.Files = append(displayPack.Files, displayFile)
	}

	// Add special files if present
	if status.IsIgnored {
		displayPack.Files = append([]display.DisplayFile{{
			Path:   ".dodotignore",
			Status: "ignored",
		}}, displayPack.Files...)
	}
	if status.HasConfig {
		displayPack.Files = append([]display.DisplayFile{{
			Path:   ".dodot.toml",
			Status: "config",
		}}, displayPack.Files...)
	}

	return displayPack
}

// statusStateToDisplayStatus converts internal status states to display status strings
func statusStateToDisplayStatus(state StatusState) string {
	switch state {
	case StatusStateReady, StatusStateSuccess:
		return "success"
	case StatusStateMissing:
		return "queue"
	case StatusStatePending:
		return "queue"
	case StatusStateError:
		return "error"
	case StatusStateIgnored:
		return "ignored"
	case StatusStateConfig:
		return "config"
	default:
		return "unknown"
	}
}
