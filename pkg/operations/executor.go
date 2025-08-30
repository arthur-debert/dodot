package operations

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// Executor orchestrates the execution of operations.
// This is where the complexity lives - handlers just declare what they want,
// the executor figures out how to make it happen.
type Executor struct {
	store      SimpleDataStore
	dryRun     bool
	confirmer  Confirmer
	fileSystem types.FS
}

// NewExecutor creates a new operation executor.
func NewExecutor(store SimpleDataStore, fs types.FS, confirmer Confirmer, dryRun bool) *Executor {
	return &Executor{
		store:      store,
		fileSystem: fs,
		confirmer:  confirmer,
		dryRun:     dryRun,
	}
}

// Execute runs a list of operations, handling validation, confirmations, and execution.
// This is the main entry point that commands use after handlers generate operations.
func (e *Executor) Execute(operations []Operation, handler Handler) ([]OperationResult, error) {
	logger := logging.GetLogger("operations.executor").With().
		Str("handler", handler.Name()).
		Int("operation_count", len(operations)).
		Bool("dry_run", e.dryRun).
		Logger()

	// Let handler validate operations if it needs to
	// For example, symlink handler checks for target conflicts
	if err := handler.ValidateOperations(operations); err != nil {
		logger.Error().Err(err).Msg("Handler validation failed")
		return nil, fmt.Errorf("validation failed: %w", err)
	}

	// Check if handler needs confirmation for these operations
	// This is mainly for destructive operations or expensive provisioning
	if metadata := handler.GetMetadata(); metadata.RequiresConfirm && !e.dryRun {
		// In the future, we could generate confirmation from operations
		// For now, handlers handle their own confirmation logic
		logger.Debug().Msg("Handler requires confirmation")
	}

	var results []OperationResult

	for _, op := range operations {
		logger.Debug().
			Str("type", operationTypeName(op.Type)).
			Str("pack", op.Pack).
			Msg("Executing operation")

		result := e.executeOne(op)
		results = append(results, result)

		// Stop on first error unless we're in dry-run mode
		if result.Error != nil && !e.dryRun {
			return results, result.Error
		}
	}

	return results, nil
}

// executeOne executes a single operation.
// This is where operations are mapped to datastore methods.
func (e *Executor) executeOne(op Operation) OperationResult {
	logger := logging.GetLogger("operations.executor")

	if e.dryRun {
		return e.simulateOperation(op)
	}

	switch op.Type {
	case CreateDataLink:
		// Link source file into datastore structure
		// This stages files for use by shell init or further operations
		path, err := e.store.CreateDataLink(op.Pack, op.Handler, op.Source)
		if err != nil {
			return OperationResult{
				Operation: op,
				Success:   false,
				Error:     fmt.Errorf("failed to create data link: %w", err),
			}
		}
		return OperationResult{
			Operation: op,
			Success:   true,
			Message:   fmt.Sprintf("Created data link: %s", path),
		}

	case CreateUserLink:
		// Create user-visible symlink
		// Only symlink handler uses this for final user-facing links
		err := e.store.CreateUserLink(op.Source, op.Target)
		if err != nil {
			return OperationResult{
				Operation: op,
				Success:   false,
				Error:     fmt.Errorf("failed to create user link: %w", err),
			}
		}
		return OperationResult{
			Operation: op,
			Success:   true,
			Message:   fmt.Sprintf("Created link: %s → %s", op.Target, op.Source),
		}

	case RunCommand:
		// Execute command and record completion
		// This is idempotent - won't re-run if sentinel exists
		err := e.store.RunAndRecord(op.Pack, op.Handler, op.Command, op.Sentinel)
		if err != nil {
			return OperationResult{
				Operation: op,
				Success:   false,
				Error:     fmt.Errorf("failed to run command: %w", err),
			}
		}
		return OperationResult{
			Operation: op,
			Success:   true,
			Message:   fmt.Sprintf("Executed: %s", op.Command),
		}

	case CheckSentinel:
		// Query if operation was already completed
		// Used for status checks and conditional execution
		exists, err := e.store.HasSentinel(op.Pack, op.Handler, op.Sentinel)
		if err != nil {
			return OperationResult{
				Operation: op,
				Success:   false,
				Error:     fmt.Errorf("failed to check sentinel: %w", err),
			}
		}
		message := "Not completed"
		if exists {
			message = "Already completed"
		}
		return OperationResult{
			Operation: op,
			Success:   true,
			Message:   message,
		}

	default:
		logger.Error().Int("type", int(op.Type)).Msg("Unknown operation type")
		return OperationResult{
			Operation: op,
			Success:   false,
			Error:     fmt.Errorf("unknown operation type: %d", op.Type),
		}
	}
}

// simulateOperation returns what would happen without actually doing it.
// This is crucial for dry-run support and user understanding.
func (e *Executor) simulateOperation(op Operation) OperationResult {
	var message string

	switch op.Type {
	case CreateDataLink:
		message = fmt.Sprintf("Would create data link: %s", op.Source)
	case CreateUserLink:
		message = fmt.Sprintf("Would create link: %s → datastore", op.Target)
	case RunCommand:
		message = fmt.Sprintf("Would execute: %s", op.Command)
	case CheckSentinel:
		message = fmt.Sprintf("Would check sentinel: %s", op.Sentinel)
	}

	return OperationResult{
		Operation: op,
		Success:   true,
		Message:   message,
	}
}

// ExecuteClear handles the clear operation for a handler.
// This demonstrates how handler customization points are used.
func (e *Executor) ExecuteClear(handler Handler, ctx types.ClearContext) ([]types.ClearedItem, error) {
	logger := logging.GetLogger("operations.executor").With().
		Str("handler", handler.Name()).
		Str("pack", ctx.Pack.Name).
		Bool("dry_run", ctx.DryRun).
		Logger()

	// Let handler decide if confirmation is needed
	// For example, homebrew checks DODOT_HOMEBREW_UNINSTALL env var
	if confirmation := handler.GetClearConfirmation(ctx); confirmation != nil {
		if !e.confirmer.RequestConfirmation(
			confirmation.ID,
			confirmation.Title,
			confirmation.Description,
			confirmation.Items...,
		) {
			logger.Info().Msg("User cancelled clear operation")
			return nil, fmt.Errorf("operation cancelled by user")
		}
	}

	// PHASE 1 IMPLEMENTATION NOTE:
	// This implementation is specific to the path handler for Phase 1.
	// In Phase 2, as more handlers are migrated, we'll extract common patterns.
	// In Phase 3, this will become a generic implementation that works with
	// the simplified datastore interface using a "RemoveState" operation.
	//
	// The current implementation demonstrates the concept: handlers declare
	// what state they maintain, and the executor handles the clearing logic.
	var clearedItems []types.ClearedItem

	// Get state directory name (handler can override)
	stateDirName := handler.GetStateDirectoryName()
	if stateDirName == "" {
		stateDirName = handler.Name()
	}

	// Phase 2: Handle each handler type
	// TODO(Phase 3): Replace with generic RemoveState operation
	switch handler.Name() {
	case HandlerPath:
		stateDir := fmt.Sprintf("~/.local/share/dodot/data/%s/%s", ctx.Pack.Name, stateDirName)

		clearedItem := types.ClearedItem{
			Type:        "path_state",
			Path:        stateDir,
			Description: "PATH entries will be removed",
		}

		// Format using handler customization
		if formatted := handler.FormatClearedItem(clearedItem, ctx.DryRun); formatted != "" {
			clearedItem.Description = formatted
		} else if ctx.DryRun {
			clearedItem.Description = "Would remove PATH entries"
		}

		clearedItems = append(clearedItems, clearedItem)

	case HandlerSymlink:
		// Symlink handler stores links in the datastore
		symlinksDir := fmt.Sprintf("~/.local/share/dodot/data/%s/symlinks", ctx.Pack.Name)

		clearedItem := types.ClearedItem{
			Type:        "symlink_state",
			Path:        symlinksDir,
			Description: "Symlinks will be removed",
		}

		// Format using handler customization
		if formatted := handler.FormatClearedItem(clearedItem, ctx.DryRun); formatted != "" {
			clearedItem.Description = formatted
		}

		clearedItems = append(clearedItems, clearedItem)

	case HandlerShell:
		// Shell handler stores scripts in the datastore
		shellDir := fmt.Sprintf("~/.local/share/dodot/data/%s/shell", ctx.Pack.Name)

		clearedItem := types.ClearedItem{
			Type:        "shell_state",
			Path:        shellDir,
			Description: "Shell profile sources will be removed",
		}

		// Format using handler customization
		if formatted := handler.FormatClearedItem(clearedItem, ctx.DryRun); formatted != "" {
			clearedItem.Description = formatted
		} else if ctx.DryRun {
			clearedItem.Description = "Would remove shell profile sources"
		}

		clearedItems = append(clearedItems, clearedItem)

	case HandlerInstall:
		// Install handler stores run records in the datastore
		installDir := fmt.Sprintf("~/.local/share/dodot/data/%s/install", ctx.Pack.Name)

		clearedItem := types.ClearedItem{
			Type:        "provision_state",
			Path:        installDir,
			Description: "Install run records will be removed",
		}

		// Format using handler customization
		if formatted := handler.FormatClearedItem(clearedItem, ctx.DryRun); formatted != "" {
			clearedItem.Description = formatted
		}

		clearedItems = append(clearedItems, clearedItem)
	}

	// Actually remove if not dry run
	if !ctx.DryRun && e.store != nil && len(clearedItems) > 0 {
		// Phase 2: Just log the removal for all handlers
		// Phase 3: Will use a generic RemoveState operation:
		//   e.store.RemoveState(ctx.Pack.Name, handler.Name())
		// This will remove all datastore entries for the handler
		logger.Debug().
			Str("handler", handler.Name()).
			Str("pack", ctx.Pack.Name).
			Msg("Removing handler state")
		// For now, we just mark it as cleared
	}

	// Phase 2: Add similar blocks for other handlers as they're migrated
	// Each handler will follow the same pattern:
	// 1. Determine what state needs clearing
	// 2. Create ClearedItem descriptions
	// 3. Perform actual removal if not dry-run

	// Phase 3: Replace all handler-specific blocks with:
	// clearedItems, err := handler.GetClearedItems(ctx)
	// if !ctx.DryRun { e.store.RemoveState(ctx.Pack.Name, handler.Name()) }

	logger.Info().
		Int("cleared_items", len(clearedItems)).
		Msg("Handler cleared")

	return clearedItems, nil
}

// operationTypeName returns a human-readable name for an operation type.
func operationTypeName(t OperationType) string {
	switch t {
	case CreateDataLink:
		return "CreateDataLink"
	case CreateUserLink:
		return "CreateUserLink"
	case RunCommand:
		return "RunCommand"
	case CheckSentinel:
		return "CheckSentinel"
	default:
		return fmt.Sprintf("Unknown(%d)", t)
	}
}
