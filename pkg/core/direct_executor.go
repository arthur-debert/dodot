package core

import (
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
	"github.com/rs/zerolog"
)

// DirectExecutorOptions contains options for the direct executor
type DirectExecutorOptions struct {
	Paths             *paths.Paths
	DryRun            bool
	Force             bool
	AllowHomeSymlinks bool
	Config            *config.Config
}

// DirectExecutor executes actions directly without intermediate Operation type
type DirectExecutor struct {
	logger            zerolog.Logger
	dryRun            bool
	force             bool
	filesystem        filesystem.FullFileSystem
	paths             *paths.Paths
	config            *config.Config
	allowHomeSymlinks bool
	enableRollback    bool
	// pathValidator removed - validation will be handled directly on Actions
}

// NewDirectExecutor creates a new direct executor
func NewDirectExecutor(opts *DirectExecutorOptions) *DirectExecutor {
	// Use PathAwareFileSystem to handle absolute paths directly
	osfs := filesystem.NewOSFileSystem("/")
	pathAwareFS := synthfs.NewPathAwareFileSystem(osfs, "/").WithAbsolutePaths()

	cfg := opts.Config
	if cfg == nil {
		cfg = config.Default()
	}

	return &DirectExecutor{
		logger:            logging.GetLogger("core.direct_executor"),
		dryRun:            opts.DryRun,
		force:             opts.Force,
		filesystem:        pathAwareFS,
		paths:             opts.Paths,
		config:            cfg,
		allowHomeSymlinks: opts.AllowHomeSymlinks,
		enableRollback:    cfg.Security.EnableRollback,
		// pathValidator removed - validation will be handled directly on Actions
	}
}

// ExecuteActions executes actions directly using synthfs
func (e *DirectExecutor) ExecuteActions(actions []types.Action) ([]types.OperationResult, error) {
	if len(actions) == 0 {
		return []types.OperationResult{}, nil
	}

	e.logger.Info().Int("actionCount", len(actions)).Msg("Executing actions directly")

	// Sort actions by priority
	sortedActions := make([]types.Action, len(actions))
	copy(sortedActions, actions)
	sort.Slice(sortedActions, func(i, j int) bool {
		if sortedActions[i].Priority != sortedActions[j].Priority {
			return sortedActions[i].Priority > sortedActions[j].Priority
		}
		if sortedActions[i].Type != sortedActions[j].Type {
			return sortedActions[i].Type < sortedActions[j].Type
		}
		return sortedActions[i].Target < sortedActions[j].Target
	})

	// For dry run, just log what would be done
	if e.dryRun {
		return e.executeDryRun(sortedActions), nil
	}

	// Create synthfs instance
	sfs := synthfs.New()
	ctx := context.Background()

	// Convert actions to synthfs operations
	synthfsOps := []synthfs.Operation{}
	actionMap := make(map[synthfs.OperationID]*types.Action)

	for i := range sortedActions {
		action := &sortedActions[i]

		ops, err := e.convertActionToSynthfsOps(sfs, *action)
		if err != nil {
			e.logger.Error().
				Err(err).
				Str("type", string(action.Type)).
				Str("description", action.Description).
				Msg("Failed to convert action")
			return nil, errors.Wrapf(err, errors.ErrActionExecute,
				"failed to convert action: %s", action.Description)
		}

		for _, op := range ops {
			synthfsOps = append(synthfsOps, op)
			actionMap[op.ID()] = action
		}
	}

	if len(synthfsOps) == 0 {
		return []types.OperationResult{}, nil
	}

	// Set up pipeline options
	options := synthfs.DefaultPipelineOptions()
	options.RollbackOnError = e.enableRollback

	// Execute all operations together
	e.logger.Info().
		Int("operationCount", len(synthfsOps)).
		Bool("rollbackEnabled", e.enableRollback).
		Msg("Executing synthfs operations")

	result, err := synthfs.RunWithOptions(ctx, e.filesystem, options, synthfsOps...)

	// Convert synthfs results to operation results
	results := e.convertResults(result, actionMap)

	if err != nil {
		return results, errors.Wrapf(err, errors.ErrActionExecute,
			"failed to execute actions")
	}

	e.logger.Info().Msg("All actions executed successfully")
	return results, nil
}

// convertActionToSynthfsOps converts a single action to synthfs operations
func (e *DirectExecutor) convertActionToSynthfsOps(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	switch action.Type {
	case types.ActionTypeLink:
		return e.convertLinkAction(sfs, action)
	case types.ActionTypeCopy:
		return e.convertCopyAction(sfs, action)
	case types.ActionTypeWrite:
		return e.convertWriteAction(sfs, action)
	case types.ActionTypeAppend:
		return e.convertAppendAction(sfs, action)
	case types.ActionTypeMkdir:
		return e.convertMkdirAction(sfs, action)
	case types.ActionTypeShellSource:
		return e.convertShellSourceAction(sfs, action)
	case types.ActionTypePathAdd:
		return e.convertPathAddAction(sfs, action)
	case types.ActionTypeRun:
		return e.convertRunAction(sfs, action)
	case types.ActionTypeBrew:
		return e.convertBrewAction(sfs, action)
	case types.ActionTypeInstall:
		return e.convertInstallAction(sfs, action)
	case types.ActionTypeTemplate:
		return e.convertTemplateAction(sfs, action)
	case types.ActionTypeRead:
		// Read actions don't produce synthfs operations
		return nil, nil
	case types.ActionTypeChecksum:
		// Checksum actions don't produce synthfs operations
		return nil, nil
	default:
		return nil, errors.Newf(errors.ErrActionInvalid, "unknown action type: %s", action.Type)
	}
}

// convertLinkAction converts a link action to synthfs operations
func (e *DirectExecutor) convertLinkAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Source == "" || action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "link action requires source and target")
	}

	source := expandHome(action.Source)
	target := expandHome(action.Target)

	// TODO: Add direct path validation for Actions (not Operations)

	deployedPath := filepath.Join(e.paths.SymlinkDir(), filepath.Base(target))

	var ops []synthfs.Operation

	// Create parent directory if needed
	targetDir := filepath.Dir(target)
	homeDir, _ := os.UserHomeDir()
	if targetDir != "." && targetDir != "/" && targetDir != homeDir {
		dirID := fmt.Sprintf("mkdir_%s_%s_%d", action.Pack, filepath.Base(targetDir), time.Now().UnixNano())
		ops = append(ops, sfs.CreateDirWithID(dirID, targetDir, 0755))
	}

	// Create deployment symlink
	deployID := fmt.Sprintf("link_deploy_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	ops = append(ops, sfs.CreateSymlinkWithID(deployID, source, deployedPath))

	// Create target symlink
	targetID := fmt.Sprintf("link_target_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	if e.force {
		ops = append(ops, sfs.CustomOperationWithID(targetID, func(ctx context.Context, fs filesystem.FileSystem) error {
			if err := fs.Remove(target); err != nil && !os.IsNotExist(err) {
				return err
			}
			return fs.Symlink(deployedPath, target)
		}))
	} else {
		ops = append(ops, sfs.CreateSymlinkWithID(targetID, deployedPath, target))
	}

	return ops, nil
}

// convertCopyAction converts a copy action to synthfs operations
func (e *DirectExecutor) convertCopyAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Source == "" || action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "copy action requires source and target")
	}

	source := expandHome(action.Source)
	target := expandHome(action.Target)

	// Validate paths
	// TODO: Add direct path validation for Actions (not Operations)

	id := fmt.Sprintf("copy_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	return []synthfs.Operation{sfs.CopyWithID(id, source, target)}, nil
}

// convertWriteAction converts a write action to synthfs operations
func (e *DirectExecutor) convertWriteAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "write action requires target")
	}

	target := expandHome(action.Target)

	// TODO: Add direct path validation for Actions (not Operations)

	mode := os.FileMode(0644)
	if action.Mode != 0 {
		mode = os.FileMode(action.Mode)
	}

	id := fmt.Sprintf("write_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	return []synthfs.Operation{sfs.CreateFileWithID(id, target, []byte(action.Content), mode)}, nil
}

// Other conversion methods would follow similar patterns...

// TODO: Implement direct Action validation (removed Operation-based validation)

// expandHome expands ~ to home directory
func expandHome(path string) string {
	if strings.HasPrefix(path, "~/") {
		home, _ := os.UserHomeDir()
		return filepath.Join(home, path[2:])
	}
	return path
}

// executeDryRun handles dry run mode
func (e *DirectExecutor) executeDryRun(actions []types.Action) []types.OperationResult {
	e.logger.Info().Msg("Dry run mode - actions would be executed:")
	results := make([]types.OperationResult, len(actions))

	for i, action := range actions {
		e.logAction(action)
		results[i] = types.OperationResult{
			Operation: &types.Operation{
				Type:        types.OperationType(action.Type),
				Description: action.Description,
				Pack:        action.Pack,
				PowerUp:     action.PowerUpName,
			},
			Status:    types.StatusReady,
			StartTime: time.Now(),
			EndTime:   time.Now(),
		}
	}

	return results
}

// logAction logs details about an action
func (e *DirectExecutor) logAction(action types.Action) {
	logger := e.logger.With().
		Str("type", string(action.Type)).
		Str("description", action.Description).
		Logger()

	switch action.Type {
	case types.ActionTypeLink:
		logger.Info().
			Str("source", action.Source).
			Str("target", action.Target).
			Msg("Would create symlink")
	case types.ActionTypeWrite:
		logger.Info().
			Str("target", action.Target).
			Int("contentLen", len(action.Content)).
			Msg("Would write file")
	case types.ActionTypeRun:
		logger.Info().
			Str("command", action.Command).
			Strs("args", action.Args).
			Msg("Would execute command")
	default:
		logger.Info().Msg("Would execute action")
	}
}

// convertResults converts synthfs results to operation results
func (e *DirectExecutor) convertResults(result *synthfs.Result, actionMap map[synthfs.OperationID]*types.Action) []types.OperationResult {
	if result == nil {
		return []types.OperationResult{}
	}

	statusMap := map[synthfs.OperationStatus]types.OperationStatus{
		synthfs.StatusSuccess:    types.StatusReady,
		synthfs.StatusFailure:    types.StatusError,
		synthfs.StatusValidation: types.StatusError,
	}

	results := []types.OperationResult{}
	operations := result.GetOperations()

	for _, opResult := range operations {
		if synthfsResult, ok := opResult.(synthfs.OperationResult); ok {
			action, exists := actionMap[synthfsResult.OperationID]
			if !exists {
				e.logger.Warn().
					Str("operationID", string(synthfsResult.OperationID)).
					Msg("Could not find action for synthfs result")
				continue
			}

			status := statusMap[synthfsResult.Status]
			if status == "" {
				status = types.StatusError
			}

			// FIXME: ARCHITECTURAL PROBLEM - Creating fake Operations defeats the purpose!
			// DirectExecutor should return PowerUpResults, not OperationResults.
			// Execution system should roll up all operation statuses to PowerUp level:
			// - If ANY operation in PowerUp fails, PowerUp fails (atomic unit)
			// - UI shows PowerUp status, not individual operation statuses
			// See docs/design/display.txxt
			// Create a pseudo-operation for the result
			// In the real implementation, we might want to refactor OperationResult
			// to work with Actions directly
			results = append(results, types.OperationResult{
				Operation: &types.Operation{
					Type:        types.OperationType(action.Type),
					Description: action.Description,
					Pack:        action.Pack,
					PowerUp:     action.PowerUpName,
					Source:      action.Source,
					Target:      action.Target,
				},
				Status:    status,
				Error:     synthfsResult.Error,
				StartTime: time.Now().Add(-synthfsResult.Duration),
				EndTime:   time.Now(),
			})
		}
	}

	return results
}

// Placeholder implementations for other action types
func (e *DirectExecutor) convertAppendAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "append action requires target")
	}

	target := expandHome(action.Target)
	// TODO: Add direct path validation for Actions (not Operations)

	// For append, we need to read existing content first
	id := fmt.Sprintf("append_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	return []synthfs.Operation{
		sfs.CustomOperationWithID(id, func(ctx context.Context, fs filesystem.FileSystem) error {
			// Read existing content
			file, err := fs.Open(target)
			if err != nil && !os.IsNotExist(err) {
				return err
			}
			var existing []byte
			if err == nil {
				defer func() { _ = file.Close() }()
				existing, err = io.ReadAll(file)
				if err != nil {
					return err
				}
			}

			// Append new content
			newContent := string(existing) + action.Content
			mode := os.FileMode(0644)
			if action.Mode != 0 {
				mode = os.FileMode(action.Mode)
			}

			return fs.WriteFile(target, []byte(newContent), mode)
		}),
	}, nil
}

func (e *DirectExecutor) convertMkdirAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "mkdir action requires target")
	}

	target := expandHome(action.Target)
	// TODO: Add direct path validation for Actions (not Operations)

	mode := os.FileMode(0755)
	if action.Mode != 0 {
		mode = os.FileMode(action.Mode)
	}

	id := fmt.Sprintf("mkdir_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	return []synthfs.Operation{sfs.CreateDirWithID(id, target, mode)}, nil
}

func (e *DirectExecutor) convertShellSourceAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "shell source action requires source")
	}

	// Write to shell init file
	shellInitFile := filepath.Join(e.paths.ShellDir(), "init.sh")
	content := fmt.Sprintf("\n# Source %s from %s\n[ -f \"%s\" ] && source \"%s\"\n",
		filepath.Base(action.Source), action.Pack, action.Source, action.Source)

	id := fmt.Sprintf("shell_source_%s_%d", action.Pack, time.Now().UnixNano())
	return []synthfs.Operation{
		sfs.CustomOperationWithID(id, func(ctx context.Context, fs filesystem.FileSystem) error {
			// Ensure shell directory exists
			if err := fs.MkdirAll(e.paths.ShellDir(), 0755); err != nil {
				return err
			}

			// Read existing content
			file, err := fs.Open(shellInitFile)
			if err != nil && !os.IsNotExist(err) {
				return err
			}
			var existing []byte
			if err == nil {
				defer func() { _ = file.Close() }()
				existing, err = io.ReadAll(file)
				if err != nil {
					return err
				}
			}

			// Append new content
			newContent := string(existing) + content
			return fs.WriteFile(shellInitFile, []byte(newContent), 0644)
		}),
	}, nil
}

func (e *DirectExecutor) convertPathAddAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "path add action requires target")
	}

	// Add to PATH in shell init file
	shellInitFile := filepath.Join(e.paths.ShellDir(), "init.sh")
	content := fmt.Sprintf("\n# Add %s to PATH from %s\nexport PATH=\"%s:$PATH\"\n",
		filepath.Base(action.Target), action.Pack, action.Target)

	id := fmt.Sprintf("path_add_%s_%d", action.Pack, time.Now().UnixNano())
	return []synthfs.Operation{
		sfs.CustomOperationWithID(id, func(ctx context.Context, fs filesystem.FileSystem) error {
			// Ensure shell directory exists
			if err := fs.MkdirAll(e.paths.ShellDir(), 0755); err != nil {
				return err
			}

			// Read existing content
			file, err := fs.Open(shellInitFile)
			if err != nil && !os.IsNotExist(err) {
				return err
			}
			var existing []byte
			if err == nil {
				defer func() { _ = file.Close() }()
				existing, err = io.ReadAll(file)
				if err != nil {
					return err
				}
			}

			// Check if path is already added
			if strings.Contains(string(existing), action.Target) {
				return nil // Already added
			}

			// Append new content
			newContent := string(existing) + content
			return fs.WriteFile(shellInitFile, []byte(newContent), 0644)
		}),
	}, nil
}

func (e *DirectExecutor) convertRunAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Command == "" {
		return nil, errors.New(errors.ErrActionInvalid, "run action requires command")
	}

	fullCommand := action.Command
	if len(action.Args) > 0 {
		quotedArgs := make([]string, len(action.Args))
		for i, arg := range action.Args {
			if strings.Contains(arg, " ") {
				quotedArgs[i] = fmt.Sprintf("%q", arg)
			} else {
				quotedArgs[i] = arg
			}
		}
		fullCommand = fmt.Sprintf("%s %s", action.Command, strings.Join(quotedArgs, " "))
	}

	id := fmt.Sprintf("run_%s_%d", action.Pack, time.Now().UnixNano())
	return []synthfs.Operation{
		sfs.ShellCommandWithID(id, fullCommand,
			synthfs.WithCaptureOutput(),
			synthfs.WithTimeout(30*time.Second)),
	}, nil
}

func (e *DirectExecutor) convertBrewAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	// Execute brew bundle
	return nil, errors.New(errors.ErrNotImplemented, "brew action not implemented in POC")
}

func (e *DirectExecutor) convertInstallAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "install action requires source")
	}

	// Install script execution - copy script and run it
	var ops []synthfs.Operation

	// Copy the script to install directory
	scriptTarget := filepath.Join(e.paths.InstallDir(), filepath.Base(action.Source))
	copyID := fmt.Sprintf("install_copy_%s_%d", action.Pack, time.Now().UnixNano())
	ops = append(ops, sfs.CopyWithID(copyID, action.Source, scriptTarget))

	// Make script executable using shell command
	chmodID := fmt.Sprintf("install_chmod_%s_%d", action.Pack, time.Now().UnixNano())
	ops = append(ops, sfs.ShellCommandWithID(chmodID, fmt.Sprintf("chmod +x %q", scriptTarget)))

	// Create sentinel file to mark as completed
	sentinelPath := e.paths.SentinelPath("install", action.Pack)
	sentinelID := fmt.Sprintf("install_sentinel_%s_%d", action.Pack, time.Now().UnixNano())
	ops = append(ops, sfs.CreateFileWithID(sentinelID, sentinelPath, []byte("completed"), 0644))

	return ops, nil
}

func (e *DirectExecutor) convertTemplateAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	// Process template and write file
	return nil, errors.New(errors.ErrNotImplemented, "template action not implemented in POC")
}
