package core

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"

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

	// Deduplicate operations (especially directory creation)
	logger.Debug().
		Int("beforeDedup", len(allOperations)).
		Msg("Operations before deduplication")
	allOperations = deduplicateOperations(allOperations)
	logger.Debug().
		Int("afterDedup", len(allOperations)).
		Msg("Operations after deduplication")

	// Resolve conflicts
	ResolveConflicts(&allOperations, ctx)

	logger.Info().Int("operationCount", len(allOperations)).Msg("Generated operations")
	return allOperations, nil
}

// ResolveConflicts checks for and resolves conflicts.
// It modifies the operations slice in place.
func ResolveConflicts(operations *[]types.Operation, ctx *ExecutionContext) {
	logger := logging.GetLogger("core.operations")
	ops := *operations
	force := ctx != nil && ctx.Force
	processedTargets := make(map[string]bool)

	for i := range ops {
		op := &ops[i]
		if op.Status != types.StatusReady {
			continue
		}

		target := filepath.Clean(op.Target)
		if target == "" {
			continue
		}

		if processedTargets[target] {
			if !force {
				op.Status = types.StatusConflict
			}
			continue
		}

		// Check for filesystem conflicts
		if op.Type == types.OperationCreateSymlink {
			if _, err := os.Lstat(op.Target); err == nil {
				if !force {
					op.Status = types.StatusConflict
					logger.Debug().
						Str("target", op.Target).
						Msg("Marking symlink operation as conflicted due to existing file")
				}
			} else if !os.IsNotExist(err) {
				op.Status = types.StatusError
				logger.Error().
					Err(err).
					Str("target", op.Target).
					Msg("Error checking symlink target")
			}
		}

		if op.Status == types.StatusReady {
			processedTargets[target] = true
		}
	}

	*operations = ops
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

	var ops []types.Operation
	var err error

	switch action.Type {
	case types.ActionTypeLink:
		ops, err = convertLinkAction(action)
	case types.ActionTypeCopy:
		ops, err = convertCopyAction(action)
	case types.ActionTypeWrite:
		ops, err = convertWriteAction(action)
	case types.ActionTypeAppend:
		ops, err = convertAppendAction(action)
	case types.ActionTypeMkdir:
		ops, err = convertMkdirAction(action)
	case types.ActionTypeShellSource:
		ops, err = convertShellSourceAction(action)
	case types.ActionTypePathAdd:
		ops, err = convertPathAddAction(action)
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

// convertLinkAction converts a link action to symlink operations
func convertLinkAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" || action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "link action requires source and target")
	}

	source := expandHome(action.Source)
	target := expandHome(action.Target)
	deployedPath := filepath.Join(paths.GetSymlinkDir(), filepath.Base(target))

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

// ... (other convert functions remain the same, but without setting status)

// convertCopyAction, convertWriteAction, etc. are not shown for brevity but are assumed
// to be updated to not set the status, as it's now handled in ConvertActionWithContext.

// resolveOperationConflicts checks for and resolves conflicts.
// It modifies the operations slice in place.
func resolveOperationConflicts(operations *[]types.Operation, ctx *ExecutionContext) {
	logger := logging.GetLogger("core.operations")
	ops := *operations
	force := ctx != nil && ctx.Force

	for i := range ops {
		op := &ops[i]
		if op.Status != types.StatusReady {
			continue
		}

		// Check for internal conflicts (multiple ops targeting the same path)
		for j := i + 1; j < len(ops); j++ {
			otherOp := &ops[j]
			if op.Target == otherOp.Target && !areOperationsCompatible([]*types.Operation{op, otherOp}) {
				if !force {
					logger.Error().
						Str("target", op.Target).
						Msg("Incompatible operations targeting the same path")
					op.Status = types.StatusConflict
					otherOp.Status = types.StatusConflict
				}
			}
		}

		// Check for filesystem conflicts (e.g., pre-existing files)
		if op.Type == types.OperationCreateSymlink {
			if _, err := os.Lstat(op.Target); err == nil {
				if !force {
					logger.Warn().
						Str("target", op.Target).
						Msg("Target file exists and --force is not used, marking as conflict")
					op.Status = types.StatusConflict
				}
			} else if !os.IsNotExist(err) {
				logger.Error().Err(err).Str("target", op.Target).Msg("Failed to check target file status")
				op.Status = types.StatusError
			}
		}
	}

	*operations = ops
}

func areOperationsCompatible(ops []*types.Operation) bool {
	if len(ops) <= 1 {
		return true
	}
	allDirCreates := true
	for _, op := range ops {
		if op.Type != types.OperationCreateDir {
			allDirCreates = false
			break
		}
	}
	return allDirCreates
}

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
		return nil, errors.New(errors.ErrActionInvalid, "install action requires checksum - ensure checksum action runs first")
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

// deduplicateOperations removes duplicate operations based on type and target.
// For operations with the same type and target, only the first occurrence is kept.
// This is particularly important for directory creation operations.
func deduplicateOperations(ops []types.Operation) []types.Operation {
	if len(ops) <= 1 {
		return ops
	}

	logger := logging.GetLogger("core.operations")
	seen := make(map[string]bool)
	result := make([]types.Operation, 0, len(ops))

	for _, op := range ops {
		// Create a key based on operation type and target
		// This ensures operations with same type and target are considered duplicates
		key := string(op.Type) + ":" + op.Target

		if !seen[key] {
			seen[key] = true
			result = append(result, op)
		} else {
			// Log when we skip a duplicate operation
			logger.Warn().
				Str("type", string(op.Type)).
				Str("target", op.Target).
				Str("description", op.Description).
				Msg("Skipping duplicate operation")
		}
	}

	return result
}
