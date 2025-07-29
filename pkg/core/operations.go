package core

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetFsOps converts actions into file system operations
func GetFsOps(actions []types.Action) ([]types.Operation, error) {
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
		ops, err := ConvertAction(action)
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
	
	logger.Info().Int("operationCount", len(allOperations)).Msg("Generated operations")
	return allOperations, nil
}

// ConvertAction converts a single action to one or more operations
func ConvertAction(action types.Action) ([]types.Operation, error) {
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
		return convertBrewAction(action)
	case types.ActionTypeInstall:
		return convertInstallAction(action)
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
	deployedPath := filepath.Join(types.GetSymlinkDir(), filepath.Base(target))

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
	deployedPath := filepath.Join(types.GetShellProfileDir(), deployedName)

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
		Target:      types.GetShellProfileDir(),
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
	deployedPath := filepath.Join(types.GetPathDir(), deployedName)

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
		Target:      types.GetPathDir(),
		Description: "Create PATH deployment directory",
	}}, ops...)

	return ops, nil
}

// expandHome expands ~ to the user's home directory
func expandHome(path string) string {
	if path == "~" {
		home, _ := os.UserHomeDir()
		return home
	}
	if strings.HasPrefix(path, "~/") {
		home, _ := os.UserHomeDir()
		return filepath.Join(home, path[2:])
	}
	return path
}

// convertBrewAction converts a brew action to operations
func convertBrewAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "brew action requires source (Brewfile path)")
	}

	// Get checksum from metadata
	checksum, ok := action.Metadata["checksum"].(string)
	if !ok || checksum == "" {
		return nil, errors.New(errors.ErrActionInvalid, "brew action requires checksum in metadata")
	}

	pack, ok := action.Metadata["pack"].(string)
	if !ok || pack == "" {
		return nil, errors.New(errors.ErrActionInvalid, "brew action requires pack in metadata")
	}

	// Create sentinel file with checksum
	sentinelPath := filepath.Join(types.GetBrewfileDir(), pack)
	
	ops := []types.Operation{
		// Ensure sentinel directory exists
		{
			Type:        types.OperationCreateDir,
			Target:      types.GetBrewfileDir(),
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

// convertInstallAction converts an install action to operations
func convertInstallAction(action types.Action) ([]types.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "install action requires source (install script path)")
	}

	// Get checksum from metadata
	checksum, ok := action.Metadata["checksum"].(string)
	if !ok || checksum == "" {
		return nil, errors.New(errors.ErrActionInvalid, "install action requires checksum in metadata")
	}

	pack, ok := action.Metadata["pack"].(string)
	if !ok || pack == "" {
		return nil, errors.New(errors.ErrActionInvalid, "install action requires pack in metadata")
	}

	// Create sentinel file with checksum
	sentinelPath := filepath.Join(types.GetInstallDir(), pack)
	
	ops := []types.Operation{
		// Ensure sentinel directory exists
		{
			Type:        types.OperationCreateDir,
			Target:      types.GetInstallDir(),
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