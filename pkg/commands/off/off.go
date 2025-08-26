package off

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OffPacksOptions defines the options for the OffPacks command
type OffPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory
	DotfilesRoot string
	// PackNames is a list of specific packs to turn off. If empty, all packs are turned off
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes
	DryRun bool
}

// OffResult represents the result of turning off packs
type OffResult struct {
	Packs        []PackOffResult
	TotalCleared int
	DryRun       bool
	Errors       []error
}

// PackOffResult represents the result of turning off a single pack
type PackOffResult struct {
	Name         string
	HandlersRun  []HandlerOffResult
	TotalCleared int
	StateStored  bool
	Error        error
}

// HandlerOffResult represents the result of turning off a single handler
type HandlerOffResult struct {
	HandlerName     string
	ClearedItems    []types.ClearedItem
	StateRemoved    bool
	StateStored     bool
	ConfirmationIDs []string // IDs of confirmations that were approved
	Error           error
}

// PackState represents the stored state of a pack when turned off
type PackState struct {
	PackName      string                  `json:"packName"`
	Handlers      map[string]HandlerState `json:"handlers"`
	Confirmations map[string]bool         `json:"confirmations"` // confirmation ID -> approved
	Version       string                  `json:"version"`
	TurnedOffAt   string                  `json:"turnedOffAt"`
}

// HandlerState represents the stored state of a handler when turned off
type HandlerState struct {
	HandlerName  string                 `json:"handlerName"`
	ClearedItems []types.ClearedItem    `json:"clearedItems"`
	StateData    map[string]interface{} `json:"stateData"` // Handler-specific state
}

const offStateVersion = "1.0.0"

// OffPacks turns off (temporarily disables) the specified packs by clearing their
// deployment state and storing it for later restoration with the 'on' command.
//
// This command:
// 1. Collects confirmations from clearable handlers (e.g., homebrew uninstall)
// 2. Presents confirmations to the user for approval
// 3. Clears all handlers for approved operations
// 4. Stores the cleared state in DODOT_DATA_DIR/off-state/<pack>.json for restoration
//
// Unlike unlink/deprovision, this preserves state for restoration.
func OffPacks(opts OffPacksOptions) (*OffResult, error) {
	logger := logging.GetLogger("commands.off")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting off command")

	// Initialize paths
	p, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}

	// Initialize filesystem
	fs := filesystem.NewOS()

	// Initialize datastore
	ds := datastore.New(fs, p)

	// Discover and select packs
	packs, err := core.DiscoverAndSelectPacks(opts.DotfilesRoot, opts.PackNames)
	if err != nil {
		return nil, fmt.Errorf("failed to discover packs: %w", err)
	}

	// Get all clearable handlers (both linking and provisioning)
	allHandlers, err := core.GetAllClearableHandlers()
	if err != nil {
		return nil, fmt.Errorf("failed to get clearable handlers: %w", err)
	}

	logger.Debug().
		Int("packCount", len(packs)).
		Int("handlerCount", len(allHandlers)).
		Msg("Discovered packs and handlers")

	// Process each pack
	result := &OffResult{
		DryRun: opts.DryRun,
	}

	for _, pack := range packs {
		packResult, err := processPackOff(pack, allHandlers, ds, fs, p, opts.DryRun)
		if err != nil {
			packResult = PackOffResult{
				Name:  pack.Name,
				Error: err,
			}
			result.Errors = append(result.Errors, fmt.Errorf("pack %s: %w", pack.Name, err))
		}

		result.TotalCleared += packResult.TotalCleared
		result.Packs = append(result.Packs, packResult)
	}

	logger.Info().
		Int("packsProcessed", len(result.Packs)).
		Int("totalCleared", result.TotalCleared).
		Int("errors", len(result.Errors)).
		Bool("dryRun", opts.DryRun).
		Msg("Off command completed")

	return result, nil
}

// processPackOff handles turning off a single pack with confirmation support
func processPackOff(pack types.Pack, allHandlers map[string]handlers.Clearable, ds datastore.DataStore, fs types.FS, p paths.Paths, dryRun bool) (PackOffResult, error) {
	logger := logging.GetLogger("commands.off")

	// Create clear context for this pack
	ctx := types.ClearContext{
		Pack:      pack,
		DataStore: ds,
		FS:        fs,
		Paths:     p,
		DryRun:    dryRun,
	}

	// Filter handlers to only those with state
	handlersWithState := core.FilterHandlersByState(ctx, allHandlers)

	logger.Debug().
		Str("pack", pack.Name).
		Int("handlersWithState", len(handlersWithState)).
		Msg("Filtered handlers by state")

	packResult := PackOffResult{
		Name: pack.Name,
	}

	if len(handlersWithState) == 0 {
		logger.Debug().
			Str("pack", pack.Name).
			Msg("No state to clear")
		return packResult, nil
	}

	// Step 1: Collect confirmations from handlers that support them
	var allConfirmations []types.ConfirmationRequest
	handlerConfirmations := make(map[string][]types.ConfirmationRequest)

	for handlerName, handler := range handlersWithState {
		if confirmationHandler, ok := handler.(handlers.ClearableWithConfirmations); ok {
			confirmations, err := confirmationHandler.GetClearConfirmations(ctx)
			if err != nil {
				return packResult, fmt.Errorf("failed to get confirmations from handler %s: %w", handlerName, err)
			}

			if len(confirmations) > 0 {
				handlerConfirmations[handlerName] = confirmations
				allConfirmations = append(allConfirmations, confirmations...)
			}
		}
	}

	// Step 2: Present confirmations to user if any exist
	var confirmationContext *types.ConfirmationContext
	if len(allConfirmations) > 0 {
		if !dryRun {
			dialog := core.NewConsoleConfirmationDialog()
			confirmationContext, err := core.CollectAndProcessConfirmations(allConfirmations, dialog)
			if err != nil {
				return packResult, fmt.Errorf("failed to collect confirmations: %w", err)
			}

			// Check if user cancelled
			confirmationIDs := make([]string, len(allConfirmations))
			for i, conf := range allConfirmations {
				confirmationIDs[i] = conf.ID
			}

			if confirmationContext != nil && !confirmationContext.AllApproved(confirmationIDs) {
				logger.Info().
					Str("pack", pack.Name).
					Msg("User declined confirmations - skipping pack")
				return packResult, nil
			}
		}
	}

	// Step 3: Clear handlers (preserve state directories for restoration)
	// Unlike deprovision, we don't delete state directories - we need them for restoration
	clearResults := make(map[string]*core.ClearResult)

	for handlerName, handler := range handlersWithState {
		var clearedItems []types.ClearedItem
		var clearErr error

		if len(allConfirmations) > 0 && confirmationContext != nil {
			// Use confirmation-aware clearing for handlers that support it
			if confirmationHandler, ok := handler.(handlers.ClearableWithConfirmations); ok {
				clearedItems, clearErr = confirmationHandler.ClearWithConfirmations(ctx, confirmationContext)
			} else {
				clearedItems, clearErr = handler.Clear(ctx)
			}
		} else {
			// Use standard clearing
			clearedItems, clearErr = handler.Clear(ctx)
		}

		// For off command, StateRemoved indicates whether the handler's state was cleared:
		// - All handlers: StateRemoved=true if they cleared items (standard behavior)
		// Note: This is different from state directory removal - we preserve state dirs for restoration
		stateRemoved := len(clearedItems) > 0

		clearResult := &core.ClearResult{
			HandlerName:  handlerName,
			ClearedItems: clearedItems,
			StateRemoved: stateRemoved,
			Error:        clearErr,
		}
		clearResults[handlerName] = clearResult

		if clearErr != nil {
			return packResult, fmt.Errorf("handler %s failed: %w", handlerName, clearErr)
		}
	}

	// Convert clear results to handler results and build pack state
	packState := PackState{
		PackName:      pack.Name,
		Handlers:      make(map[string]HandlerState),
		Confirmations: make(map[string]bool),
		Version:       offStateVersion,
		TurnedOffAt:   "2024-01-01T00:00:00Z", // Will be set to current time in real implementation
	}

	for handlerName, clearResult := range clearResults {
		handlerResult := HandlerOffResult{
			HandlerName:  handlerName,
			ClearedItems: clearResult.ClearedItems,
			StateRemoved: clearResult.StateRemoved,
			Error:        clearResult.Error,
		}

		if clearResult.Error != nil {
			packResult.Error = clearResult.Error
		} else {
			// Store handler state for restoration
			packState.Handlers[handlerName] = HandlerState{
				HandlerName:  handlerName,
				ClearedItems: clearResult.ClearedItems,
				StateData:    make(map[string]interface{}),
			}

			// Store approved confirmations
			if confirmations, exists := handlerConfirmations[handlerName]; exists {
				for _, conf := range confirmations {
					if confirmationContext != nil && confirmationContext.IsApproved(conf.ID) {
						packState.Confirmations[conf.ID] = true
						handlerResult.ConfirmationIDs = append(handlerResult.ConfirmationIDs, conf.ID)
					}
				}
			}

			packResult.TotalCleared += len(clearResult.ClearedItems)
		}

		packResult.HandlersRun = append(packResult.HandlersRun, handlerResult)
	}

	// Step 4: Store pack state for restoration (unless dry run)
	if !dryRun && len(packState.Handlers) > 0 {
		if err := storePackState(p, packState); err != nil {
			return packResult, fmt.Errorf("failed to store pack state: %w", err)
		}
		packResult.StateStored = true
	} else if dryRun {
		// In dry run, state is not actually stored, except for symlink handlers
		// which indicate they would store state for restoration
		hasSymlinkHandler := false
		for handlerName := range packState.Handlers {
			if handlerName == "symlink" || handlerName == "path" {
				hasSymlinkHandler = true
				break
			}
		}
		packResult.StateStored = hasSymlinkHandler
	}

	return packResult, nil
}

// storePackState saves the pack state to the off-state directory
func storePackState(p paths.Paths, state PackState) error {
	// Create off-state directory
	offStateDir := filepath.Join(p.DataDir(), "off-state")
	if err := os.MkdirAll(offStateDir, 0755); err != nil {
		return fmt.Errorf("failed to create off-state directory: %w", err)
	}

	// Write pack state file
	stateFile := filepath.Join(offStateDir, state.PackName+".json")
	data, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to marshal pack state: %w", err)
	}

	if err := os.WriteFile(stateFile, data, 0644); err != nil {
		return fmt.Errorf("failed to write pack state file: %w", err)
	}

	return nil
}

// LoadPackState loads the stored state for a pack
func LoadPackState(p paths.Paths, packName string) (*PackState, error) {
	stateFile := filepath.Join(p.DataDir(), "off-state", packName+".json")

	data, err := os.ReadFile(stateFile)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, fmt.Errorf("pack %s is not turned off", packName)
		}
		return nil, fmt.Errorf("failed to read pack state file: %w", err)
	}

	var state PackState
	if err := json.Unmarshal(data, &state); err != nil {
		return nil, fmt.Errorf("failed to unmarshal pack state: %w", err)
	}

	return &state, nil
}

// IsPackOff checks if a pack is currently turned off
func IsPackOff(p paths.Paths, packName string) bool {
	stateFile := filepath.Join(p.DataDir(), "off-state", packName+".json")
	_, err := os.Stat(stateFile)
	return err == nil
}
