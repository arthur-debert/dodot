package core

import (
	"fmt"
	"path/filepath"
	"sort"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ResolveConflicts checks for and marks conflicting operations
// This is a wrapper to maintain backward compatibility with ExecutionContext
func ResolveConflicts(ops *[]types.Operation, ctx *ExecutionContext) {
	if ctx == nil {
		operations.ResolveConflicts(ops, nil)
	} else {
		operations.ResolveConflicts(ops, ctx)
	}
}

// resolveOperationConflicts is a wrapper for the operations package function
func resolveOperationConflicts(ops *[]types.Operation, ctx *ExecutionContext) {
	operations.ResolveOperationConflicts(ops, ctx)
}

// ConvertActionsToOperations converts actions into file system operations
// This is the planning phase - no actual file system changes are made.
// DEPRECATED: Use ConvertActionsToOperationsWithContext instead.
func ConvertActionsToOperations(actions []types.Action) ([]types.Operation, error) {
	return ConvertActionsToOperationsWithContext(actions, nil)
}

// ConvertActionsToOperationsWithContext converts actions into file system operations with execution context
// This is the planning phase - no actual file system changes are made.
func ConvertActionsToOperationsWithContext(actions []types.Action, ctx *ExecutionContext) ([]types.Operation, error) {
	logger := logging.GetLogger("core.operations")
	logger.Debug().Int("actionCount", len(actions)).Msg("Converting actions to operations (planning phase)")

	if len(actions) == 0 {
		return nil, nil
	}

	// Sort actions by priority for consistent execution order
	sortedActions := make([]types.Action, len(actions))
	copy(sortedActions, actions)
	sort.Slice(sortedActions, func(i, j int) bool {
		// Higher priority first
		if sortedActions[i].Priority != sortedActions[j].Priority {
			return sortedActions[i].Priority > sortedActions[j].Priority
		}
		// Then by type for consistency
		if sortedActions[i].Type != sortedActions[j].Type {
			return sortedActions[i].Type < sortedActions[j].Type
		}
		// Finally by target path
		return sortedActions[i].Target < sortedActions[j].Target
	})

	var allOperations []types.Operation

	// Convert each action to operations
	for _, action := range sortedActions {
		ops, err := ConvertActionWithContext(action, ctx)
		if err != nil {
			logger.Error().
				Err(err).
				Str("action_type", string(action.Type)).
				Str("description", action.Description).
				Msg("Failed to convert action")
			return nil, err
		}
		allOperations = append(allOperations, ops...)
	}

	// Deduplicate operations (especially directory creation)
	logger.Debug().
		Int("beforeDedup", len(allOperations)).
		Msg("Operations before deduplication")
	allOperations = operations.DeduplicateOperations(allOperations)
	logger.Debug().
		Int("afterDedup", len(allOperations)).
		Msg("Operations after deduplication")

	// Resolve conflicts
	ResolveConflicts(&allOperations, ctx)

	logger.Info().Int("operationCount", len(allOperations)).Msg("Converted actions to operations (ready for execution)")
	return allOperations, nil
}

// ResolveConflicts checks for and resolves conflicts.
// It modifies the operations slice in place.

// ConvertAction converts a single action to one or more operations
// DEPRECATED: Use ConvertActionWithContext instead.
func ConvertAction(action types.Action) ([]types.Operation, error) {
	return ConvertActionWithContext(action, nil)
}

// ConvertActionWithContext converts a single action to one or more operations with execution context
func ConvertActionWithContext(action types.Action, ctx *ExecutionContext) ([]types.Operation, error) {
	logger := logging.GetLogger("core.operations").With().
		Str("type", string(action.Type)).
		Str("description", action.Description).
		Logger()

	logger.Debug().Msg("Converting action to operations")

	var ops []types.Operation
	var err error

	switch action.Type {
	case types.ActionTypeLink:
		ops, err = convertLinkActionWithContext(action, ctx)
	case types.ActionTypeCopy:
		ops, err = convertCopyAction(action)
	case types.ActionTypeWrite:
		ops, err = convertWriteAction(action)
	case types.ActionTypeAppend:
		ops, err = convertAppendAction(action)
	case types.ActionTypeMkdir:
		ops, err = convertMkdirAction(action)
	case types.ActionTypeShellSource:
		ops, err = convertShellSourceActionWithContext(action, ctx)
	case types.ActionTypePathAdd:
		ops, err = convertPathAddActionWithContext(action, ctx)
	case types.ActionTypeRun:
		logger.Debug().Msg("Run actions are not converted to file operations")
		return nil, nil
	case types.ActionTypeBrew:
		if ctx == nil {
			logger.Debug().Msg("Skipping brew action without context")
			return nil, nil
		}
		ops, err = convertBrewActionWithContext(action, ctx)
	case types.ActionTypeInstall:
		if ctx == nil {
			logger.Debug().Msg("Skipping install action without context")
			return nil, nil
		}
		ops, err = convertInstallActionWithContext(action, ctx)
	case types.ActionTypeRead:
		ops, err = convertReadAction(action)
	case types.ActionTypeChecksum:
		ops, err = convertChecksumAction(action)
	default:
		return nil, errors.Newf(errors.ErrActionInvalid, "unknown action type: %s", action.Type)
	}

	if err != nil {
		return nil, err
	}

	// Set default status for all new operations
	for i := range ops {
		ops[i].Status = types.StatusReady
	}

	return ops, nil
}

// convertLinkActionWithContext converts a link action to symlink operations
func convertLinkActionWithContext(action types.Action, ctx *ExecutionContext) ([]types.Operation, error) {
	if action.Source == "" || action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "link action requires source and target")
	}

	if ctx == nil || ctx.Paths == nil {
		return nil, errors.New(errors.ErrActionInvalid, "link action requires execution context with paths")
	}

	source := operations.ExpandHome(action.Source)
	target := operations.ExpandHome(action.Target)
	deployedPath := filepath.Join(ctx.Paths.SymlinkDir(), filepath.Base(target))

	ops := []types.Operation{
		{
			Type:        types.OperationCreateSymlink,
			Source:      source,
			Target:      deployedPath,
			Description: fmt.Sprintf("Deploy symlink for %s", filepath.Base(target)),
		},
		{
			Type:        types.OperationCreateSymlink,
			Source:      deployedPath,
			Target:      target,
			Description: action.Description,
		},
	}

	targetDir := filepath.Dir(target)
	if targetDir != "." && targetDir != "/" {
		ops = append([]types.Operation{{
			Type:        types.OperationCreateDir,
			Target:      targetDir,
			Description: fmt.Sprintf("Create parent directory for %s", filepath.Base(target)),
		}}, ops...)
	}

	return ops, nil
}

// to be updated to not set the status, as it's now handled in ConvertActionWithContext.

// resolveOperationConflicts checks for and resolves conflicts.
// It modifies the operations slice in place.

// NOTE: The rest of the file (various convert functions) is omitted for brevity.
// They are assumed to be present and correct. I will only show the changed parts.
// The key change is removing the error return from detectOperationConflicts and
// modifying operations in-place.
// I'm also adding the filesystem check.
// I will now stub out the other functions to make the replacement valid.

// convertCopyAction converts a copy action to copy operations
func convertCopyAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" || action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "copy action requires source and target")
	}

	source := operations.ExpandHome(action.Source)
	target := operations.ExpandHome(action.Target)

	ops := []types.Operation{
		{
			Type:        types.OperationCopyFile,
			Source:      source,
			Target:      target,
			Description: action.Description,
		},
	}

	// Create parent directory if needed
	targetDir := filepath.Dir(target)
	if targetDir != "." && targetDir != "/" {
		ops = append([]types.Operation{{
			Type:        types.OperationCreateDir,
			Target:      targetDir,
			Description: fmt.Sprintf("Create parent directory for %s", filepath.Base(target)),
		}}, ops...)
	}

	return ops, nil
}

// convertWriteAction converts a write action to write operations
func convertWriteAction(action types.Action) ([]types.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "write action requires target")
	}

	target := operations.ExpandHome(action.Target)

	// Convert mode to pointer if non-zero
	var mode *uint32
	if action.Mode != 0 {
		m := action.Mode
		mode = &m
	}

	ops := []types.Operation{
		{
			Type:        types.OperationWriteFile,
			Target:      target,
			Content:     action.Content,
			Mode:        mode,
			Description: action.Description,
		},
	}

	// Create parent directory if needed
	targetDir := filepath.Dir(target)
	if targetDir != "." && targetDir != "/" {
		ops = append([]types.Operation{{
			Type:        types.OperationCreateDir,
			Target:      targetDir,
			Description: fmt.Sprintf("Create parent directory for %s", filepath.Base(target)),
		}}, ops...)
	}

	return ops, nil
}

// convertAppendAction converts an append action to operations
func convertAppendAction(action types.Action) ([]types.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "append action requires target")
	}

	// For append, we need to read existing content and write combined
	// Since we're not doing actual FS operations, we'll create a write operation
	// with a note that it should append
	target := operations.ExpandHome(action.Target)

	ops := []types.Operation{
		{
			Type:        types.OperationWriteFile,
			Target:      target,
			Content:     action.Content, // In real execution, this would be appended
			Description: fmt.Sprintf("Append to %s", action.Target),
		},
	}

	return ops, nil
}

// convertMkdirAction converts a mkdir action to operations
func convertMkdirAction(action types.Action) ([]types.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "mkdir action requires target")
	}

	target := operations.ExpandHome(action.Target)

	var mode *uint32
	if action.Mode != 0 {
		m := action.Mode
		mode = &m
	}

	ops := []types.Operation{
		{
			Type:        types.OperationCreateDir,
			Target:      target,
			Mode:        mode,
			Description: action.Description,
		},
	}

	return ops, nil
}

// convertShellSourceActionWithContext converts shell source action to operations with execution context
func convertShellSourceActionWithContext(action types.Action, ctx *ExecutionContext) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "shell_source action requires source")
	}

	if ctx == nil || ctx.Paths == nil {
		return nil, errors.New(errors.ErrActionInvalid, "shell_source action requires execution context with paths")
	}

	// Create symlink in shell_profile deployment directory
	source := operations.ExpandHome(action.Source)
	deployedName := fmt.Sprintf("%s.sh", action.Pack)
	if action.Pack == "" {
		deployedName = filepath.Base(source)
	}
	deployedPath := filepath.Join(ctx.Paths.ShellProfileDir(), deployedName)

	ops := []types.Operation{
		{
			Type:        types.OperationCreateSymlink,
			Source:      source,
			Target:      deployedPath,
			Description: fmt.Sprintf("Deploy shell profile script from %s", action.Pack),
		},
	}

	// Ensure deployment directory exists
	ops = append([]types.Operation{{
		Type:        types.OperationCreateDir,
		Target:      ctx.Paths.ShellProfileDir(),
		Description: "Create shell profile deployment directory",
	}}, ops...)

	return ops, nil
}

// convertPathAddActionWithContext converts path add action to operations with execution context
func convertPathAddActionWithContext(action types.Action, ctx *ExecutionContext) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "path_add action requires source")
	}

	if ctx == nil || ctx.Paths == nil {
		return nil, errors.New(errors.ErrActionInvalid, "path_add action requires execution context with paths")
	}

	// Create symlink in path deployment directory
	source := operations.ExpandHome(action.Source)
	deployedName := action.Pack
	if deployedName == "" {
		deployedName = filepath.Base(source)
	}
	deployedPath := filepath.Join(ctx.Paths.PathDir(), deployedName)

	ops := []types.Operation{
		{
			Type:        types.OperationCreateSymlink,
			Source:      source,
			Target:      deployedPath,
			Description: fmt.Sprintf("Add %s to PATH", deployedName),
		},
	}

	// Ensure deployment directory exists
	ops = append([]types.Operation{{
		Type:        types.OperationCreateDir,
		Target:      ctx.Paths.PathDir(),
		Description: "Create PATH deployment directory",
	}}, ops...)

	return ops, nil
}

// expandHome expands ~ to the user's home directory

// convertBrewActionWithContext converts a brew action to operations with execution context
func convertBrewActionWithContext(action types.Action, ctx *ExecutionContext) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "brew action requires source (Brewfile path)")
	}

	// Get checksum from execution context or metadata
	var checksum string
	if ctx != nil {
		if cs, exists := ctx.GetChecksum(action.Source); exists {
			checksum = cs
		}
	}

	// Fall back to metadata if no context or checksum not found
	if checksum == "" {
		if cs, ok := action.Metadata["checksum"].(string); ok {
			checksum = cs
		}
	}

	// If still no checksum, this is an error - checksum actions should have run first
	if checksum == "" {
		return nil, errors.New(errors.ErrActionInvalid, "install action requires checksum - ensure checksum action runs first")
	}

	pack, ok := action.Metadata["pack"].(string)
	if !ok || pack == "" {
		return nil, errors.New(errors.ErrActionInvalid, "brew action requires pack in metadata")
	}

	// Create sentinel file with checksum
	sentinelPath := ctx.Paths.SentinelPath("homebrew", pack)

	ops := []types.Operation{
		// Ensure sentinel directory exists
		{
			Type:        types.OperationCreateDir,
			Target:      ctx.Paths.HomebrewDir(),
			Description: "Create brewfile sentinel directory",
		},
		// Execute brew bundle command
		{
			Type:        types.OperationExecute,
			Command:     "brew",
			Args:        []string{"bundle", "--file", action.Source},
			WorkingDir:  filepath.Dir(action.Source),
			Description: fmt.Sprintf("Execute brew bundle for %s", pack),
		},
		// Write sentinel file with checksum only after successful execution
		{
			Type:        types.OperationWriteFile,
			Target:      sentinelPath,
			Content:     checksum,
			Mode:        operations.Uint32Ptr(uint32(config.Default().FilePermissions.File)),
			Description: fmt.Sprintf("Create brewfile sentinel for %s", pack),
		},
	}

	return ops, nil
}

// convertInstallActionWithContext converts an install action to operations with execution context
func convertInstallActionWithContext(action types.Action, ctx *ExecutionContext) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "install action requires source (install script path)")
	}

	// Get checksum from execution context or metadata
	var checksum string
	if ctx != nil {
		if cs, exists := ctx.GetChecksum(action.Source); exists {
			checksum = cs
		}
	}

	// Fall back to metadata if no context or checksum not found
	if checksum == "" {
		if cs, ok := action.Metadata["checksum"].(string); ok {
			checksum = cs
		}
	}

	// If still no checksum, this is an error - checksum actions should have run first
	if checksum == "" {
		return nil, errors.New(errors.ErrActionInvalid, "install action requires checksum - ensure checksum action runs first")
	}

	pack, ok := action.Metadata["pack"].(string)
	if !ok || pack == "" {
		return nil, errors.New(errors.ErrActionInvalid, "install action requires pack in metadata")
	}

	// Create sentinel file with checksum
	sentinelPath := ctx.Paths.SentinelPath("install", pack)

	ops := []types.Operation{
		// Ensure sentinel directory exists
		{
			Type:        types.OperationCreateDir,
			Target:      ctx.Paths.InstallDir(),
			Description: "Create install sentinel directory",
		},
		// Execute the install script
		{
			Type:        types.OperationExecute,
			Command:     "/bin/sh",
			Args:        []string{action.Source},
			WorkingDir:  filepath.Dir(action.Source),
			Description: fmt.Sprintf("Execute install script for %s", pack),
		},
		// Write sentinel file with checksum only after successful execution
		{
			Type:        types.OperationWriteFile,
			Target:      sentinelPath,
			Content:     checksum,
			Mode:        operations.Uint32Ptr(uint32(config.Default().FilePermissions.File)),
			Description: fmt.Sprintf("Create install sentinel for %s", pack),
		},
	}

	return ops, nil
}

// Helper to create uint32 pointer

// convertReadAction converts a read action to read operations
func convertReadAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "read action requires source")
	}

	source := operations.ExpandHome(action.Source)

	ops := []types.Operation{
		{
			Type:        types.OperationReadFile,
			Source:      source,
			Description: fmt.Sprintf("Read file %s", filepath.Base(source)),
		},
	}

	return ops, nil
}

// convertChecksumAction converts a checksum action to checksum operations
func convertChecksumAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "checksum action requires source")
	}

	source := operations.ExpandHome(action.Source)

	ops := []types.Operation{
		{
			Type:        types.OperationChecksum,
			Source:      source,
			Description: fmt.Sprintf("Calculate checksum for %s", filepath.Base(source)),
		},
	}

	return ops, nil
}

// deduplicateOperations removes duplicate operations based on type and target.
// For operations with the same type and target, only the first occurrence is kept.
// This is particularly important for directory creation operations.
