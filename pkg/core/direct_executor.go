package core

import (
	"bytes"
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
func (e *DirectExecutor) ExecuteActions(actions []types.Action) ([]types.ActionResult, error) {
	if len(actions) == 0 {
		return []types.ActionResult{}, nil
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

		// Validate action before processing
		if err := e.validateAction(*action); err != nil {
			e.logger.Error().
				Err(err).
				Str("type", string(action.Type)).
				Str("description", action.Description).
				Msg("Action failed validation")
			return nil, errors.Wrapf(err, errors.ErrActionInvalid,
				"action validation failed: %s", action.Description)
		}

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
		return []types.ActionResult{}, nil
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

	// Convert synthfs results to action results
	results := e.convertResults(result, actionMap)

	if err != nil {
		return results, errors.Wrapf(err, errors.ErrActionExecute,
			"failed to execute actions")
	}

	e.logger.Info().Msg("All actions executed successfully")

	// Write deployment metadata after successful execution
	if !e.dryRun && len(results) > 0 {
		if err := e.writeDeploymentMetadata(); err != nil {
			// Log error but don't fail the deployment
			e.logger.Warn().Err(err).Msg("Failed to write deployment metadata")
		}
	}

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
	case types.ActionTypeRead:
		return e.convertReadAction(sfs, action)
	case types.ActionTypeChecksum:
		return e.convertChecksumAction(sfs, action)
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

	// Path validation is now handled by validateAction in ExecuteActions

	deployedPath := filepath.Join(e.paths.SymlinkDir(), filepath.Base(target))

	var ops []synthfs.Operation

	// Create parent directory if needed
	targetDir := filepath.Dir(target)
	homeDir, _ := os.UserHomeDir()
	if targetDir != "." && targetDir != "/" && targetDir != homeDir {
		dirID := fmt.Sprintf("mkdir_%s_%s_%d", action.Pack, filepath.Base(targetDir), time.Now().UnixNano())
		ops = append(ops, sfs.CreateDirWithID(dirID, targetDir, 0755))
	}

	// STEP 1: Create deployment symlink (intermediate link in data directory)
	// This implements dodot's two-symlink strategy:
	//   1. Intermediate symlink: ~/.local/share/dodot/deployed/symlink/.vimrc -> ~/dotfiles/vim/vimrc
	//   2. Target symlink: ~/.vimrc -> ~/.local/share/dodot/deployed/symlink/.vimrc
	//
	// This intermediate link is ALWAYS safe to update because:
	// - It's in dodot's private data directory
	// - No user files exist here
	// - It allows atomic updates without touching user's home directory
	//
	// The operation is idempotent - repeated deploys will:
	// - Do nothing if the link already points to the correct source
	// - Update the link if it points to a different source (pack was moved)
	deployID := fmt.Sprintf("link_deploy_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	ops = append(ops, sfs.CustomOperationWithID(deployID, func(ctx context.Context, fs filesystem.FileSystem) error {
		// Ensure the deployed directory exists
		deployedDir := filepath.Dir(deployedPath)
		if err := fs.MkdirAll(deployedDir, 0755); err != nil {
			return fmt.Errorf("failed to create deployed directory %s: %w", deployedDir, err)
		}

		// Check if the symlink already exists and points to the same source
		if existingTarget, err := fs.Readlink(deployedPath); err == nil {
			if existingTarget == source {
				// Already correct, nothing to do - idempotent operation
				return nil
			}
			// Points to different source - remove and recreate
			// This handles the case where a pack was moved to a different location
			if err := fs.Remove(deployedPath); err != nil {
				return fmt.Errorf("failed to remove existing deployment symlink: %w", err)
			}
		}
		// Create the symlink
		return fs.Symlink(source, deployedPath)
	}))

	// STEP 2: Create target symlink (the actual symlink in user's home or target location)
	// This is where conflict resolution happens. The strategy is:
	//
	// 1. If target is already a symlink:
	//    - If it points to our intermediate link: do nothing (idempotent)
	//    - If it points elsewhere: conflict (require --force)
	//
	// 2. If target is a regular file:
	//    - Compare content with source file
	//    - If identical: auto-replace with symlink (safe operation)
	//    - If different: conflict (require --force to avoid data loss)
	//
	// 3. If target doesn't exist: create the symlink
	//
	// This approach ensures:
	// - Repeated deploys are idempotent (no errors on re-run)
	// - User data is protected (conflicts require explicit --force)
	// - Common case of identical files is handled automatically
	targetID := fmt.Sprintf("link_target_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	ops = append(ops, sfs.CustomOperationWithID(targetID, func(ctx context.Context, fs filesystem.FileSystem) error {
		// Check if target already exists
		if existingTarget, err := fs.Readlink(target); err == nil {
			// CASE 1: Target is a symlink
			// Check if it points to our deployed path
			e.logger.Debug().
				Str("existingTarget", existingTarget).
				Str("deployedPath", deployedPath).
				Msg("Checking if symlink already points to correct location")

			// Compare paths - readlink might return relative or absolute paths
			if existingTarget == deployedPath {
				// Already correct, nothing to do - idempotent operation
				return nil
			}

			// Handle path format differences (readlink sometimes omits leading /)
			// This is a quirk of some filesystem implementations
			if !filepath.IsAbs(existingTarget) && !strings.HasPrefix(existingTarget, "/") {
				// Check if adding / makes it match
				if "/"+existingTarget == deployedPath {
					return nil
				}
			}

			// Symlink points somewhere else - this is a conflict
			// User must explicitly use --force to overwrite
			if !e.force {
				return fmt.Errorf("target %s already exists and points to %s (use --force to overwrite)", target, existingTarget)
			}
		} else if targetFile, openErr := fs.Open(target); openErr == nil {
			// CASE 2: Target is a regular file
			// We can open it, but it's not a symlink
			_ = targetFile.Close()
			if !e.force {
				// Smart conflict resolution: check if file content is identical
				// This handles the common case where user has the same config file
				// but not yet managed by dodot
				sourceContent, readErr := e.readFileContent(ctx, fs, source)
				if readErr == nil {
					targetContent, targetReadErr := e.readFileContent(ctx, fs, target)
					if targetReadErr == nil && bytes.Equal(sourceContent, targetContent) {
						// Content is identical - safe to replace with symlink
						// This is a quality-of-life feature that makes adoption easier
						e.logger.Debug().Str("target", target).Msg("Target file has identical content, replacing with symlink")
					} else {
						// Content differs - protect user data
						return fmt.Errorf("target %s already exists as a regular file (use --force to overwrite)", target)
					}
				}
			}
		}
		// CASE 3: Target doesn't exist, or we're forcing, or content matches

		// Remove existing file/symlink if needed
		// This is safe because we've already checked conflicts above
		if err := fs.Remove(target); err != nil && !os.IsNotExist(err) {
			return fmt.Errorf("failed to remove existing target: %w", err)
		}

		// Create the symlink pointing to our intermediate link
		// This completes the two-symlink chain:
		// ~/.vimrc -> ~/.local/share/dodot/deployed/symlink/.vimrc -> ~/dotfiles/vim/vimrc
		return fs.Symlink(deployedPath, target)
	}))

	return ops, nil
}

// convertCopyAction converts a copy action to synthfs operations
func (e *DirectExecutor) convertCopyAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Source == "" || action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "copy action requires source and target")
	}

	source := expandHome(action.Source)
	target := expandHome(action.Target)

	// Path validation is now handled by validateAction in ExecuteActions

	id := fmt.Sprintf("copy_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	return []synthfs.Operation{sfs.CopyWithID(id, source, target)}, nil
}

// convertWriteAction converts a write action to synthfs operations
func (e *DirectExecutor) convertWriteAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "write action requires target")
	}

	target := expandHome(action.Target)

	// Path validation is now handled by validateAction in ExecuteActions

	mode := os.FileMode(0644)
	if action.Mode != 0 {
		mode = os.FileMode(action.Mode)
	}

	id := fmt.Sprintf("write_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	return []synthfs.Operation{sfs.CreateFileWithID(id, target, []byte(action.Content), mode)}, nil
}

// Other conversion methods would follow similar patterns...

// Action validation is implemented in validateAction method

// expandHome expands ~ to home directory
func expandHome(path string) string {
	if strings.HasPrefix(path, "~/") {
		home, _ := os.UserHomeDir()
		return filepath.Join(home, path[2:])
	}
	return path
}

// executeDryRun handles dry run mode
func (e *DirectExecutor) executeDryRun(actions []types.Action) []types.ActionResult {
	e.logger.Info().Msg("Dry run mode - actions would be executed:")
	results := make([]types.ActionResult, len(actions))

	now := time.Now()
	for i, action := range actions {
		e.logAction(action)
		message := e.generateActionMessage(&action, types.StatusReady, nil)
		results[i] = types.ActionResult{
			Action:    action,
			Status:    types.StatusReady,
			StartTime: now,
			EndTime:   now,
			Message:   message,
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

// convertResults converts synthfs results to action results
func (e *DirectExecutor) convertResults(result *synthfs.Result, actionMap map[synthfs.OperationID]*types.Action) []types.ActionResult {
	if result == nil {
		return []types.ActionResult{}
	}

	statusMap := map[synthfs.OperationStatus]types.OperationStatus{
		synthfs.StatusSuccess:    types.StatusReady,
		synthfs.StatusFailure:    types.StatusError,
		synthfs.StatusValidation: types.StatusError,
	}

	// Group operations by action to create proper ActionResults
	actionResults := make(map[*types.Action]*types.ActionResult)
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

			// Get or create ActionResult for this action
			if actionResult, exists := actionResults[action]; exists {
				// Update existing result - if any operation fails, the entire action fails
				if status == types.StatusError {
					actionResult.Status = types.StatusError
					if actionResult.Error == nil {
						actionResult.Error = synthfsResult.Error
					}
				}
				// Track all synthfs operation IDs for debugging
				actionResult.SynthfsOperationIDs = append(actionResult.SynthfsOperationIDs, string(synthfsResult.OperationID))
			} else {
				// Create new ActionResult
				now := time.Now()
				message := e.generateActionMessage(action, status, synthfsResult.Error)
				actionResults[action] = &types.ActionResult{
					Action:              *action,
					Status:              status,
					Error:               synthfsResult.Error,
					Message:             message,
					StartTime:           now.Add(-synthfsResult.Duration),
					EndTime:             now,
					SynthfsOperationIDs: []string{string(synthfsResult.OperationID)},
				}
			}
		}
	}

	// Convert map to slice
	results := make([]types.ActionResult, 0, len(actionResults))
	for _, actionResult := range actionResults {
		results = append(results, *actionResult)
	}

	return results
}

// generateActionMessage creates user-friendly messages based on action type and status
func (e *DirectExecutor) generateActionMessage(action *types.Action, status types.OperationStatus, err error) string {
	// If there's an error, return a contextual error message
	if err != nil {
		return e.generateErrorMessage(action, err)
	}

	// Generate success messages based on action type
	switch action.Type {
	case types.ActionTypeLink:
		if status == types.StatusReady {
			return fmt.Sprintf("linked to %s", filepath.Base(action.Target))
		}
		return fmt.Sprintf("prepared symlink to %s", filepath.Base(action.Target))

	case types.ActionTypeCopy:
		if status == types.StatusReady {
			return fmt.Sprintf("copied to %s", filepath.Base(action.Target))
		}
		return fmt.Sprintf("prepared copy to %s", filepath.Base(action.Target))

	case types.ActionTypeWrite:
		if status == types.StatusReady {
			return fmt.Sprintf("wrote %s", filepath.Base(action.Target))
		}
		return fmt.Sprintf("prepared write to %s", filepath.Base(action.Target))

	case types.ActionTypeAppend:
		if status == types.StatusReady {
			return fmt.Sprintf("appended to %s", filepath.Base(action.Target))
		}
		return fmt.Sprintf("prepared append to %s", filepath.Base(action.Target))

	case types.ActionTypeMkdir:
		if status == types.StatusReady {
			return fmt.Sprintf("created directory %s", filepath.Base(action.Target))
		}
		return fmt.Sprintf("prepared directory %s", filepath.Base(action.Target))

	case types.ActionTypeShellSource:
		if status == types.StatusReady {
			return "added to shell profile"
		}
		return "prepared shell profile update"

	case types.ActionTypePathAdd:
		if status == types.StatusReady {
			return fmt.Sprintf("added %s to PATH", filepath.Base(action.Target))
		}
		return fmt.Sprintf("prepared PATH addition for %s", filepath.Base(action.Target))

	case types.ActionTypeRun:
		if status == types.StatusReady {
			return "executed successfully"
		}
		return "prepared for execution"

	case types.ActionTypeBrew:
		if status == types.StatusReady {
			return "Homebrew packages installed"
		}
		return "prepared Homebrew installation"

	case types.ActionTypeInstall:
		if status == types.StatusReady {
			return "install script executed"
		}
		return "prepared install script"

	default:
		if status == types.StatusReady {
			return "completed successfully"
		}
		return "prepared"
	}
}

// generateErrorMessage creates user-friendly error messages based on action type
func (e *DirectExecutor) generateErrorMessage(action *types.Action, err error) string {
	baseMsg := ""
	switch action.Type {
	case types.ActionTypeLink:
		baseMsg = fmt.Sprintf("failed to create symlink to %s", filepath.Base(action.Target))
	case types.ActionTypeCopy:
		baseMsg = fmt.Sprintf("failed to copy to %s", filepath.Base(action.Target))
	case types.ActionTypeWrite:
		baseMsg = fmt.Sprintf("failed to write %s", filepath.Base(action.Target))
	case types.ActionTypeAppend:
		baseMsg = fmt.Sprintf("failed to append to %s", filepath.Base(action.Target))
	case types.ActionTypeMkdir:
		baseMsg = fmt.Sprintf("failed to create directory %s", filepath.Base(action.Target))
	case types.ActionTypeShellSource:
		baseMsg = "failed to update shell profile"
	case types.ActionTypePathAdd:
		baseMsg = fmt.Sprintf("failed to add %s to PATH", filepath.Base(action.Target))
	case types.ActionTypeRun:
		baseMsg = "command execution failed"
	case types.ActionTypeBrew:
		baseMsg = "Homebrew installation failed"
	case types.ActionTypeInstall:
		baseMsg = "install script failed"
	default:
		baseMsg = "action failed"
	}

	// Add error details if available
	if err != nil {
		return fmt.Sprintf("%s: %v", baseMsg, err)
	}
	return baseMsg
}

// convertAppendAction converts an append action to synthfs operations
func (e *DirectExecutor) convertAppendAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "append action requires target")
	}

	target := expandHome(action.Target)
	// Path validation is now handled by validateAction in ExecuteActions

	// For append, we need to read existing content first
	id := fmt.Sprintf("append_%s_%s_%d", action.Pack, filepath.Base(target), time.Now().UnixNano())
	mode := os.FileMode(0644)
	if action.Mode != 0 {
		mode = os.FileMode(action.Mode)
	}

	return []synthfs.Operation{
		sfs.CustomOperationWithID(id, e.createAppendFileOperation(target, action.Content, mode)),
	}, nil
}

func (e *DirectExecutor) convertMkdirAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "mkdir action requires target")
	}

	target := expandHome(action.Target)
	// Path validation is now handled by validateAction in ExecuteActions

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
		sfs.CustomOperationWithID(id, e.createAppendFileOperation(shellInitFile, content, 0644)),
	}, nil
}

func (e *DirectExecutor) convertPathAddAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Target == "" {
		return nil, errors.New(errors.ErrActionInvalid, "path add action requires target")
	}

	var ops []synthfs.Operation

	// First, create a symlink in deployed/path directory
	// The symlink name should be <pack>-<dirname> (e.g., "tools-bin")
	dirName := filepath.Base(action.Target)
	if action.Metadata != nil {
		if dn, ok := action.Metadata["dirName"].(string); ok {
			dirName = dn
		}
	}

	deployedPathDir := filepath.Join(e.paths.DeployedDir(), "path")
	symlinkName := fmt.Sprintf("%s-%s", action.Pack, dirName)
	symlinkPath := filepath.Join(deployedPathDir, symlinkName)

	// Create the symlink to the original directory (idempotent)
	linkID := fmt.Sprintf("path_link_%s_%s_%d", action.Pack, dirName, time.Now().UnixNano())
	ops = append(ops, sfs.CustomOperationWithID(linkID, func(ctx context.Context, fs filesystem.FileSystem) error {
		// Check if symlink already exists and points to correct target
		if existingTarget, err := fs.Readlink(symlinkPath); err == nil {
			if existingTarget == action.Target {
				// Symlink already exists and is correct
				return nil
			}
			// Remove incorrect symlink
			if err := fs.Remove(symlinkPath); err != nil {
				return err
			}
		}

		// Create parent directory if needed
		parentDir := filepath.Dir(symlinkPath)
		if err := fs.MkdirAll(parentDir, 0755); err != nil {
			return err
		}

		// Create the symlink
		return fs.Symlink(action.Target, symlinkPath)
	}))

	// Then add to PATH in shell init file using the deployed symlink path
	shellInitFile := filepath.Join(e.paths.ShellDir(), "init.sh")
	content := fmt.Sprintf("\n# Add %s to PATH from %s\nexport PATH=\"%s:$PATH\"\n",
		dirName, action.Pack, symlinkPath)

	appendID := fmt.Sprintf("path_append_%s_%d", action.Pack, time.Now().UnixNano())
	ops = append(ops, sfs.CustomOperationWithID(appendID, func(ctx context.Context, fs filesystem.FileSystem) error {
		// First check if path is already added
		file, err := fs.Open(shellInitFile)
		if err == nil {
			defer func() { _ = file.Close() }()
			existing, err := io.ReadAll(file)
			if err == nil && strings.Contains(string(existing), symlinkPath) {
				return nil // Already added
			}
		}

		// Use the helper to append
		return e.createAppendFileOperation(shellInitFile, content, 0644)(ctx, fs)
	}))

	return ops, nil
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
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "brew action requires source Brewfile")
	}

	// Brew actions execute 'brew bundle --file=<brewfile>'
	// This installs/updates packages from a Brewfile
	brewfile := expandHome(action.Source)

	// Path validation is now handled by validateAction in ExecuteActions

	// Execute brew bundle command
	command := fmt.Sprintf("brew bundle --file=%q", brewfile)
	id := fmt.Sprintf("brew_%s_%d", action.Pack, time.Now().UnixNano())

	ops := []synthfs.Operation{
		sfs.ShellCommandWithID(id, command,
			synthfs.WithCaptureOutput(),
			synthfs.WithTimeout(300*time.Second)), // 5 minutes for brew operations
	}

	// Create sentinel file to mark as completed (for run-once behavior)
	if action.Pack != "" {
		sentinelPath := e.paths.SentinelPath("homebrew", action.Pack)
		sentinelID := fmt.Sprintf("brew_sentinel_%s_%d", action.Pack, time.Now().UnixNano())

		// Write checksum from metadata if available
		checksumContent := "completed"
		if action.Metadata != nil {
			if checksum, ok := action.Metadata["checksum"].(string); ok && checksum != "" {
				checksumContent = checksum
			}
		}

		ops = append(ops, sfs.CreateFileWithID(sentinelID, sentinelPath, []byte(checksumContent), 0644))
	}

	return ops, nil
}

func (e *DirectExecutor) convertInstallAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "install action requires source")
	}

	// Install script execution - copy script and run it
	var ops []synthfs.Operation

	// Copy the script to install directory (include pack name to avoid conflicts)
	scriptTarget := filepath.Join(e.paths.InstallDir(), action.Pack, filepath.Base(action.Source))
	copyID := fmt.Sprintf("install_copy_%s_%d", action.Pack, time.Now().UnixNano())

	if e.force {
		// If force is enabled, use custom operation to handle existing files
		ops = append(ops, sfs.CustomOperationWithID(copyID, func(ctx context.Context, fs filesystem.FileSystem) error {
			// Remove existing file if it exists
			if err := fs.Remove(scriptTarget); err != nil && !os.IsNotExist(err) {
				return err
			}
			// Copy the file
			return copyFile(fs, action.Source, scriptTarget)
		}))
	} else {
		ops = append(ops, sfs.CopyWithID(copyID, action.Source, scriptTarget))
	}

	// Make script executable using shell command
	chmodID := fmt.Sprintf("install_chmod_%s_%d", action.Pack, time.Now().UnixNano())
	ops = append(ops, sfs.ShellCommandWithID(chmodID, fmt.Sprintf("chmod +x %q", scriptTarget)))

	// Execute the install script with environment variables
	execID := fmt.Sprintf("install_exec_%s_%d", action.Pack, time.Now().UnixNano())

	// Set up environment variables for the script
	envVars := map[string]string{
		"DOTFILES_ROOT":  e.paths.DotfilesRoot(),
		"DODOT_DATA_DIR": e.paths.DataDir(),
		"DODOT_PACK":     action.Pack,
	}

	// Add HOME from environment if available
	if home := os.Getenv("HOME"); home != "" {
		envVars["HOME"] = home
	}

	// Build the command with environment variables
	var envCmd strings.Builder
	for k, v := range envVars {
		envCmd.WriteString(fmt.Sprintf("%s=%q ", k, v))
	}
	envCmd.WriteString(scriptTarget)

	ops = append(ops, sfs.ShellCommandWithID(execID, envCmd.String(),
		synthfs.WithCaptureOutput(),
		synthfs.WithTimeout(300*time.Second))) // 5 minutes for install scripts

	// Create sentinel file to mark as completed
	sentinelPath := e.paths.SentinelPath("install", action.Pack)
	sentinelID := fmt.Sprintf("install_sentinel_%s_%d", action.Pack, time.Now().UnixNano())

	// Get checksum from metadata if available
	checksumContent := "completed"
	if action.Metadata != nil {
		if checksum, ok := action.Metadata["checksum"].(string); ok && checksum != "" {
			checksumContent = checksum
		}
	}

	if e.force {
		// If force is enabled, use custom operation to overwrite sentinel file
		ops = append(ops, sfs.CustomOperationWithID(sentinelID, func(ctx context.Context, fs filesystem.FileSystem) error {
			// Ensure parent directory exists
			sentinelDir := filepath.Dir(sentinelPath)
			if err := fs.MkdirAll(sentinelDir, 0755); err != nil {
				return err
			}
			// Remove existing sentinel if it exists
			if err := fs.Remove(sentinelPath); err != nil && !os.IsNotExist(err) {
				return err
			}
			// Create new sentinel
			return fs.WriteFile(sentinelPath, []byte(checksumContent), 0644)
		}))
	} else {
		ops = append(ops, sfs.CreateFileWithID(sentinelID, sentinelPath, []byte(checksumContent), 0644))
	}

	return ops, nil
}

// validateAction performs comprehensive validation on an Action before execution
func (e *DirectExecutor) validateAction(action types.Action) error {
	// Check action type-specific validation
	switch action.Type {
	case types.ActionTypeLink:
		return e.validateLinkAction(action.Source, action.Target)
	case types.ActionTypeCopy:
		return e.validateCopyAction(action.Source, action.Target)
	case types.ActionTypeWrite, types.ActionTypeAppend:
		// Special case: shell_profile power-ups can append to shell config files in home
		// even when home symlinks are not generally allowed
		if action.Type == types.ActionTypeAppend && action.PowerUpName == "shell_profile" {
			// Only validate that it's not a protected system file
			return e.validateNotSystemFile(action.Target)
		}
		return e.validateWriteAction(action.Target)
	case types.ActionTypeMkdir:
		return e.validateMkdirAction(action.Target)
	case types.ActionTypeRun:
		// Command execution doesn't need path validation
		return nil
	case types.ActionTypeBrew, types.ActionTypeInstall:
		// These have their own safety mechanisms (sentinel files, checksums)
		return nil
	case types.ActionTypeShellSource, types.ActionTypePathAdd:
		// These write to dodot-controlled directories
		return nil
	case types.ActionTypeRead, types.ActionTypeChecksum:
		// Read-only operations are safe
		return nil
	default:
		return errors.Newf(errors.ErrActionInvalid, "unknown action type for validation: %s", action.Type)
	}
}

// validateLinkAction validates paths for link actions
func (e *DirectExecutor) validateLinkAction(source, target string) error {
	// Validate source exists in dotfiles or deployed directory
	sourceInDotfiles := isPathWithin(source, e.paths.DotfilesRoot())
	sourceInDeployed := isPathWithin(source, e.paths.DeployedDir())

	if !sourceInDotfiles && !sourceInDeployed {
		return errors.Newf(errors.ErrInvalidInput, "source path %s is outside dotfiles or deployed directory", source)
	}

	// Check if target is a protected system file
	if err := e.validateNotSystemFile(target); err != nil {
		return err
	}

	// If home symlinks are not allowed, check if target is in home directory
	if !e.allowHomeSymlinks {
		homeDir, err := os.UserHomeDir()
		if err != nil {
			return errors.Wrapf(err, errors.ErrFileAccess, "failed to get home directory")
		}

		// Check if target is outside dodot-controlled directories
		dodotDataDir := e.paths.DataDir()
		dodotSymlinkDir := e.paths.SymlinkDir()

		// Allow targets in dodot directories
		if isPathWithin(target, dodotDataDir) || isPathWithin(target, dodotSymlinkDir) {
			return nil
		}

		// Check if target is in home directory
		if isPathWithin(target, homeDir) {
			return errors.Newf(errors.ErrInvalidInput, "target path %s is outside dodot-controlled directories", target)
		}
	}

	return nil
}

// validateCopyAction validates paths for copy actions
func (e *DirectExecutor) validateCopyAction(source, target string) error {
	// Source should be from dotfiles or deployed directory
	sourceInDotfiles := isPathWithin(source, e.paths.DotfilesRoot())
	sourceInDeployed := isPathWithin(source, e.paths.DeployedDir())

	if !sourceInDotfiles && !sourceInDeployed {
		return errors.Newf(errors.ErrInvalidInput, "source path %s is outside dotfiles or deployed directory", source)
	}

	// Target should be in a safe location
	return e.validateSafePath(target)
}

// validateWriteAction validates target path for write/append actions
func (e *DirectExecutor) validateWriteAction(target string) error {
	// Check if target is a protected system file
	if err := e.validateNotSystemFile(target); err != nil {
		return err
	}

	// Ensure target is in a safe location
	return e.validateSafePath(target)
}

// validateMkdirAction validates target path for mkdir actions
func (e *DirectExecutor) validateMkdirAction(target string) error {
	// Directories should only be created in safe locations
	return e.validateSafePath(target)
}

// validateSafePath ensures operations only occur in dodot-controlled directories
func (e *DirectExecutor) validateSafePath(path string) error {
	// Normalize the path
	path = expandHome(path)
	absPath, err := filepath.Abs(path)
	if err != nil {
		return errors.Wrapf(err, errors.ErrInvalidInput, "invalid path: %s", path)
	}

	// Get safe directories
	safeDirectories := []string{
		e.paths.DotfilesRoot(), // Allow operations in dotfiles root
		e.paths.DataDir(),
		e.paths.ConfigDir(),
		e.paths.CacheDir(),
		e.paths.StateDir(),
		e.paths.SymlinkDir(),
		e.paths.InstallDir(),
		e.paths.ShellDir(),
		e.paths.DeployedDir(),
		e.paths.BackupsDir(),
		e.paths.HomebrewDir(),
		e.paths.TemplatesDir(),
	}

	// Resolve symlinks for comparison (handles macOS /var -> /private/var)
	resolvedPath, err := filepath.EvalSymlinks(absPath)
	if err != nil && !os.IsNotExist(err) {
		// If we can't resolve, use the original path
		resolvedPath = absPath
	} else if err == nil {
		// Successfully resolved
		absPath = resolvedPath
	}

	// Check if path is within any safe directory
	for _, safeDir := range safeDirectories {
		if isPathWithin(absPath, safeDir) {
			return nil
		}
		// Also check the unresolved path in case of symlinks
		if resolvedPath != absPath && isPathWithin(path, safeDir) {
			return nil
		}
	}

	// If home symlinks are allowed, home directory is also safe
	if e.allowHomeSymlinks {
		homeDir, err := os.UserHomeDir()
		if err != nil {
			return errors.Wrapf(err, errors.ErrFileAccess, "failed to get home directory")
		}
		if isPathWithin(absPath, homeDir) {
			return nil
		}
	}

	return errors.Newf(errors.ErrInvalidInput, "path %s is outside dodot-controlled directories", path)
}

// isPathWithin checks if a path is within a parent directory
// This handles edge cases like symlinks and relative paths properly
func isPathWithin(path, parent string) bool {
	// Normalize both paths
	path = filepath.Clean(path)
	parent = filepath.Clean(parent)

	// Get relative path
	rel, err := filepath.Rel(parent, path)
	if err != nil {
		return false
	}

	// If relative path starts with ".." or is absolute, it's outside parent
	return !strings.HasPrefix(rel, "..") && !filepath.IsAbs(rel)
}

// validateNotSystemFile prevents overwriting critical system files
func (e *DirectExecutor) validateNotSystemFile(path string) error {
	// Normalize the path
	path = expandHome(path)

	// Get home directory for relative checks
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return errors.Wrapf(err, errors.ErrFileAccess, "failed to get home directory")
	}

	// Check against protected paths from config
	for protectedPath := range e.config.Security.ProtectedPaths {
		// Convert protected path to absolute
		checkPath := protectedPath
		if !filepath.IsAbs(checkPath) {
			checkPath = filepath.Join(homeDir, checkPath)
		}

		// Check if the target path matches or is within a protected path
		if path == checkPath || isPathWithin(path, checkPath) {
			return errors.Newf(errors.ErrInvalidInput,
				"cannot modify protected system file: %s", protectedPath)
		}
	}

	return nil
}

// convertReadAction converts a read action to synthfs operations
func (e *DirectExecutor) convertReadAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "read action requires source")
	}

	source := expandHome(action.Source)

	// Create read operation
	id := fmt.Sprintf("read_%s_%s_%d", action.Pack, filepath.Base(source), time.Now().UnixNano())
	return []synthfs.Operation{sfs.ReadFileWithID(id, source)}, nil
}

// convertChecksumAction converts a checksum action to synthfs operations
func (e *DirectExecutor) convertChecksumAction(sfs *synthfs.SynthFS, action types.Action) ([]synthfs.Operation, error) {
	if action.Source == "" {
		return nil, errors.New(errors.ErrActionInvalid, "checksum action requires source")
	}

	source := expandHome(action.Source)

	// Get algorithm from metadata, default to SHA256
	algorithm := synthfs.SHA256
	if action.Metadata != nil {
		if alg, ok := action.Metadata["algorithm"].(string); ok {
			switch strings.ToLower(alg) {
			case "md5":
				algorithm = synthfs.MD5
			case "sha1":
				algorithm = synthfs.SHA1
			case "sha256":
				algorithm = synthfs.SHA256
			case "sha512":
				algorithm = synthfs.SHA512
			}
		}
	}

	// Create checksum operation
	id := fmt.Sprintf("checksum_%s_%s_%d", action.Pack, filepath.Base(source), time.Now().UnixNano())
	return []synthfs.Operation{sfs.ChecksumWithID(id, source, algorithm)}, nil
}

// createAppendFileOperation creates a reusable function for appending content to files
func (e *DirectExecutor) createAppendFileOperation(target, content string, mode os.FileMode) func(context.Context, filesystem.FileSystem) error {
	return func(ctx context.Context, fs filesystem.FileSystem) error {
		// Ensure parent directory exists
		parentDir := filepath.Dir(target)
		if parentDir != "." && parentDir != "/" {
			if err := fs.MkdirAll(parentDir, 0755); err != nil {
				return fmt.Errorf("failed to create parent directory %s: %w", parentDir, err)
			}
		}

		// Read existing content
		file, err := fs.Open(target)
		if err != nil && !os.IsNotExist(err) {
			return fmt.Errorf("failed to open file %s: %w", target, err)
		}
		var existing []byte
		if err == nil {
			defer func() { _ = file.Close() }()
			existing, err = io.ReadAll(file)
			if err != nil {
				return fmt.Errorf("failed to read file %s: %w", target, err)
			}
		}

		// Append new content
		newContent := string(existing) + content
		err = fs.WriteFile(target, []byte(newContent), mode)
		if err != nil {
			return fmt.Errorf("failed to write file %s: %w", target, err)
		}

		return nil
	}
}

// copyFile copies a file from source to destination using the filesystem interface
func copyFile(fs filesystem.FileSystem, source, destination string) error {
	// Ensure destination directory exists
	destDir := filepath.Dir(destination)
	if destDir != "." && destDir != "/" {
		if err := fs.MkdirAll(destDir, 0755); err != nil {
			return fmt.Errorf("failed to create destination directory %s: %w", destDir, err)
		}
	}

	// Open source file
	srcFile, err := fs.Open(source)
	if err != nil {
		return fmt.Errorf("failed to open source file %s: %w", source, err)
	}
	defer func() { _ = srcFile.Close() }()

	// Read source content
	content, err := io.ReadAll(srcFile)
	if err != nil {
		return fmt.Errorf("failed to read source file %s: %w", source, err)
	}

	// Get source file info for permissions (if filesystem supports Stat)
	var mode os.FileMode = 0644 // Default permissions
	if fullFS, ok := fs.(filesystem.FullFileSystem); ok {
		if srcInfo, err := fullFS.Stat(source); err == nil {
			mode = srcInfo.Mode()
		}
	}

	// Write to destination
	err = fs.WriteFile(destination, content, mode)
	if err != nil {
		return fmt.Errorf("failed to write destination file %s: %w", destination, err)
	}

	return nil
}

// readFileContent reads the content of a file using the filesystem interface
func (e *DirectExecutor) readFileContent(ctx context.Context, fs filesystem.FileSystem, path string) ([]byte, error) {
	file, err := fs.Open(path)
	if err != nil {
		return nil, err
	}
	defer func() { _ = file.Close() }()

	return io.ReadAll(file)
}

// writeDeploymentMetadata writes deployment metadata for the shell init script to use
func (e *DirectExecutor) writeDeploymentMetadata() error {
	metadataPath := filepath.Join(e.paths.DataDir(), "deployment-metadata")

	// Create the metadata content
	// We use DODOT_DEPLOYMENT_ROOT to avoid confusion with DOTFILES_ROOT
	// which might be set by the user
	content := fmt.Sprintf("# Generated by dodot during deployment\n"+
		"# This file is sourced by dodot-init.sh\n"+
		"DODOT_DEPLOYMENT_ROOT=\"%s\"\n", e.paths.DotfilesRoot())

	// Write the file
	return e.filesystem.WriteFile(metadataPath, []byte(content), 0644)
}
