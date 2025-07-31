package core

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetFileOperations converts actions into file system operations
func GetFileOperations(actions []types.Action) ([]types.Operation, error) {
	return GetFileOperationsWithContext(actions, nil)
}

// GetFileOperationsWithContext converts actions into file system operations with execution context
func GetFileOperationsWithContext(actions []types.Action, ctx *ExecutionContext) ([]types.Operation, error) {
	logger := logging.GetLogger("core.operations")
	logger.Debug().Int("actionCount", len(actions)).Msg("Converting actions to operations")

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

	// Check for conflicts across all operations
	if err := detectOperationConflicts(allOperations); err != nil {
		return nil, err
	}

	logger.Info().Int("operationCount", len(allOperations)).Msg("Generated operations")
	return allOperations, nil
}

// ConvertAction converts a single action to one or more operations
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

	switch action.Type {
	case types.ActionTypeLink:
		return convertLinkAction(action)
	case types.ActionTypeCopy:
		return convertCopyAction(action)
	case types.ActionTypeWrite:
		return convertWriteAction(action)
	case types.ActionTypeAppend:
		return convertAppendAction(action)
	case types.ActionTypeMkdir:
		return convertMkdirAction(action)
	case types.ActionTypeShellSource:
		return convertShellSourceAction(action)
	case types.ActionTypePathAdd:
		return convertPathAddAction(action)
	case types.ActionTypeRun:
		// Run actions don't convert to file operations
		// They're handled separately during execution
		logger.Debug().Msg("Run actions are not converted to file operations")
		return nil, nil
	case types.ActionTypeBrew:
		return convertBrewActionWithContext(action, ctx)
	case types.ActionTypeInstall:
		return convertInstallActionWithContext(action, ctx)
	case types.ActionTypeRead:
		return convertReadAction(action)
	case types.ActionTypeChecksum:
		return convertChecksumAction(action)
	default:
		return nil, errors.Newf(errors.ErrActionInvalid, "unknown action type: %s", action.Type)
	}
}

// convertLinkAction converts a link action to symlink operations
func convertLinkAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" || action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "link action requires source and target")
	}

	// Expand home directory in paths
	source := expandHome(action.Source)
	target := expandHome(action.Target)

	// For double-symlink approach, create two operations:
	// 1. Link from deployed dir to source
	// 2. Link from target to deployed dir
	deployedPath := filepath.Join(paths.GetSymlinkDir(), filepath.Base(target))

	ops := []types.Operation{
		// First create symlink in deployed directory
		{
			Type:        types.OperationCreateSymlink,
			Source:      source,
			Target:      deployedPath,
			Description: fmt.Sprintf("Deploy symlink for %s", filepath.Base(target)),
		},
		// Then create symlink from user location to deployed
		{
			Type:        types.OperationCreateSymlink,
			Source:      deployedPath,
			Target:      target,
			Description: action.Description,
		},
	}

	// If target directory doesn't exist, create it first
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

// convertCopyAction converts a copy action to copy operations
func convertCopyAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" || action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "copy action requires source and target")
	}

	source := expandHome(action.Source)
	target := expandHome(action.Target)

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

	target := expandHome(action.Target)

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
	target := expandHome(action.Target)

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

	target := expandHome(action.Target)

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

// convertShellSourceAction converts shell source action to operations
func convertShellSourceAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "shell_source action requires source")
	}

	// Create symlink in shell_profile deployment directory
	source := expandHome(action.Source)
	deployedName := fmt.Sprintf("%s.sh", action.Pack)
	if action.Pack == "" {
		deployedName = filepath.Base(source)
	}
	deployedPath := filepath.Join(paths.GetShellProfileDir(), deployedName)

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
		Target:      paths.GetShellProfileDir(),
		Description: "Create shell profile deployment directory",
	}}, ops...)

	return ops, nil
}

// convertPathAddAction converts path add action to operations
func convertPathAddAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "path_add action requires source")
	}

	// Create symlink in path deployment directory
	source := expandHome(action.Source)
	deployedName := action.Pack
	if deployedName == "" {
		deployedName = filepath.Base(source)
	}
	deployedPath := filepath.Join(paths.GetPathDir(), deployedName)

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
		Target:      paths.GetPathDir(),
		Description: "Create PATH deployment directory",
	}}, ops...)

	return ops, nil
}

// expandHome expands ~ to the user's home directory
func expandHome(path string) string {
	return paths.ExpandHome(path)
}

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
		return nil, errors.New(errors.ErrActionInvalid, "brew action requires checksum - ensure checksum action runs first")
	}

	pack, ok := action.Metadata["pack"].(string)
	if !ok || pack == "" {
		return nil, errors.New(errors.ErrActionInvalid, "brew action requires pack in metadata")
	}

	// Create sentinel file with checksum
	sentinelPath := filepath.Join(paths.GetBrewfileDir(), pack)

	ops := []types.Operation{
		// Ensure sentinel directory exists
		{
			Type:        types.OperationCreateDir,
			Target:      paths.GetBrewfileDir(),
			Description: "Create brewfile sentinel directory",
		},
		// Write sentinel file with checksum
		{
			Type:        types.OperationWriteFile,
			Target:      sentinelPath,
			Content:     checksum,
			Mode:        uint32Ptr(0644),
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
	sentinelPath := filepath.Join(paths.GetInstallDir(), pack)

	ops := []types.Operation{
		// Ensure sentinel directory exists
		{
			Type:        types.OperationCreateDir,
			Target:      paths.GetInstallDir(),
			Description: "Create install sentinel directory",
		},
		// Write sentinel file with checksum
		{
			Type:        types.OperationWriteFile,
			Target:      sentinelPath,
			Content:     checksum,
			Mode:        uint32Ptr(0644),
			Description: fmt.Sprintf("Create install sentinel for %s", pack),
		},
	}

	return ops, nil
}

// Helper to create uint32 pointer
func uint32Ptr(v uint32) *uint32 {
	return &v
}

// detectOperationConflicts checks for conflicts between operations targeting the same paths
func detectOperationConflicts(operations []types.Operation) error {
	logger := logging.GetLogger("core.operations")

	// Track targets by path
	// Map of target path -> list of operations targeting it
	targetMap := make(map[string][]types.Operation)

	for _, op := range operations {
		// Skip operations without targets (shouldn't happen but be safe)
		if op.Target == "" {
			continue
		}

		// Normalize the target path
		target := filepath.Clean(op.Target)

		// Add to target map
		targetMap[target] = append(targetMap[target], op)
	}

	// Check for conflicts
	var conflicts []string
	for target, ops := range targetMap {
		if len(ops) <= 1 {
			continue
		}

		// Multiple operations targeting the same path
		// Check if they're compatible
		if !areOperationsCompatible(ops) {
			// Build conflict message
			var descriptions []string
			for _, op := range ops {
				descriptions = append(descriptions, fmt.Sprintf("%s (%s)", op.Description, op.Type))
			}

			conflict := fmt.Sprintf("Multiple operations target %s: %s",
				target, strings.Join(descriptions, ", "))
			conflicts = append(conflicts, conflict)

			logger.Error().
				Str("target", target).
				Int("operation_count", len(ops)).
				Strs("operations", descriptions).
				Msg("Detected operation conflict")
		}
	}

	if len(conflicts) > 0 {
		return errors.New(errors.ErrActionConflict,
			fmt.Sprintf("Detected %d conflicts:\n%s",
				len(conflicts), strings.Join(conflicts, "\n")))
	}

	return nil
}

// areOperationsCompatible checks if multiple operations targeting the same path are compatible
func areOperationsCompatible(ops []types.Operation) bool {
	// Single operation is always compatible with itself
	if len(ops) <= 1 {
		return true
	}

	// Multiple directory creation operations are compatible
	allDirCreates := true
	for _, op := range ops {
		if op.Type != types.OperationCreateDir {
			allDirCreates = false
			break
		}
	}
	if allDirCreates {
		return true
	}

	// All other combinations are incompatible
	// This includes:
	// - Multiple symlinks to same target
	// - Symlink and write to same target
	// - Multiple writes to same target
	// - Copy and write to same target
	// etc.
	return false
}

// convertReadAction converts a read action to read operations
func convertReadAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "read action requires source")
	}

	source := expandHome(action.Source)

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

	source := expandHome(action.Source)

	ops := []types.Operation{
		{
			Type:        types.OperationChecksum,
			Source:      source,
			Description: fmt.Sprintf("Calculate checksum for %s", filepath.Base(source)),
		},
	}

	return ops, nil
}
