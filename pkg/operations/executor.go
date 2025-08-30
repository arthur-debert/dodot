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

	// In a real implementation, we would:
	// 1. Read handler state from datastore
	// 2. Generate reverse operations
	// 3. Execute them
	// 4. Return cleared items

	// For now, return empty to show the structure
	return []types.ClearedItem{}, nil
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
