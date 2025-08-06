package synthfs

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
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

// SynthfsExecutor executes dodot operations using synthfs
type SynthfsExecutor struct {
	logger            zerolog.Logger
	dryRun            bool
	force             bool
	filesystem        filesystem.FullFileSystem
	paths             *paths.Paths
	allowHomeSymlinks bool
	backupExisting    bool
	enableRollback    bool
	config            *config.Config
}

// NewSynthfsExecutor creates a new synthfs-based executor
func NewSynthfsExecutor(dryRun bool) *SynthfsExecutor {
	// Initialize paths with empty string to use defaults
	p, _ := paths.New("")
	// Use PathAwareFileSystem to handle absolute paths directly
	osfs := filesystem.NewOSFileSystem("/")
	pathAwareFS := synthfs.NewPathAwareFileSystem(osfs, "/").WithAbsolutePaths()
	cfg := config.Default()

	return &SynthfsExecutor{
		logger:            logging.GetLogger("core.synthfs"),
		dryRun:            dryRun,
		filesystem:        pathAwareFS,
		paths:             p,
		allowHomeSymlinks: cfg.Security.AllowHomeSymlinks,
		backupExisting:    cfg.Security.BackupExisting,
		enableRollback:    cfg.Security.EnableRollback,
		config:            cfg,
	}
}

// NewSynthfsExecutorWithPaths creates a new synthfs-based executor with custom paths
func NewSynthfsExecutorWithPaths(dryRun bool, p *paths.Paths) *SynthfsExecutor {
	// Use PathAwareFileSystem to handle absolute paths directly
	osfs := filesystem.NewOSFileSystem("/")
	pathAwareFS := synthfs.NewPathAwareFileSystem(osfs, "/").WithAbsolutePaths()
	cfg := config.Default()

	return &SynthfsExecutor{
		logger:            logging.GetLogger("core.synthfs"),
		dryRun:            dryRun,
		filesystem:        pathAwareFS,
		paths:             p,
		allowHomeSymlinks: cfg.Security.AllowHomeSymlinks,
		backupExisting:    cfg.Security.BackupExisting,
		enableRollback:    cfg.Security.EnableRollback,
		config:            cfg,
	}
}

// EnableHomeSymlinks allows the executor to create symlinks in the user's home directory
// This should be used with caution and only for SymlinkPowerUp
func (e *SynthfsExecutor) EnableHomeSymlinks(backup bool) *SynthfsExecutor {
	e.allowHomeSymlinks = true
	e.backupExisting = backup
	return e
}

// EnableForce enables or disables force mode (overwrite existing files)
func (e *SynthfsExecutor) EnableForce(force bool) *SynthfsExecutor {
	e.force = force
	return e
}

// EnableRollback enables or disables automatic rollback on errors
func (e *SynthfsExecutor) EnableRollback(enable bool) *SynthfsExecutor {
	e.enableRollback = enable
	return e
}

// ExecuteOperations executes a list of operations using synthfs
func (e *SynthfsExecutor) ExecuteOperations(ops []types.Operation) ([]types.OperationResult, error) {
	if len(ops) == 0 {
		return []types.OperationResult{}, nil
	}

	// For dry run, just log what would be done
	if e.dryRun {
		return e.executeDryRun(ops), nil
	}

	// Create synthfs instance
	sfs := synthfs.New()
	ctx := context.Background()

	// Convert dodot operations to synthfs operations
	synthfsOps := []synthfs.Operation{}
	dodotOpMap := make(map[synthfs.OperationID]*types.Operation)
	skippedResults := []types.OperationResult{}

	for i := range ops {
		op := &ops[i]

		// Handle non-ready operations
		if op.Status != types.StatusReady {
			skippedResults = append(skippedResults, types.OperationResult{
				Operation: op,
				Status:    op.Status,
				StartTime: time.Now(),
				EndTime:   time.Now(),
			})
			continue
		}

		// Convert to synthfs operation
		synthfsOp, err := e.convertToSynthfsOp(sfs, *op)
		if err != nil {
			e.logger.Error().
				Err(err).
				Str("type", string(op.Type)).
				Str("target", op.Target).
				Msg("Failed to convert operation")
			return nil, errors.Wrapf(err, errors.ErrActionExecute,
				"failed to convert operation: %s", op.Description)
		}

		if synthfsOp != nil {
			synthfsOps = append(synthfsOps, synthfsOp)
			dodotOpMap[synthfsOp.ID()] = op
		}
	}

	// If no operations to execute, return skipped results
	if len(synthfsOps) == 0 {
		return skippedResults, nil
	}

	// Set up pipeline options
	options := synthfs.DefaultPipelineOptions()
	options.RollbackOnError = e.enableRollback

	// Execute all operations together
	e.logger.Info().
		Int("operationCount", len(synthfsOps)).
		Bool("rollbackEnabled", e.enableRollback).
		Msg("Executing operations")

	result, err := synthfs.RunWithOptions(ctx, e.filesystem, options, synthfsOps...)

	// Convert synthfs results to dodot results
	results := e.convertResults(result, dodotOpMap)

	// Prepend skipped results
	results = append(skippedResults, results...)

	if err != nil {
		return results, errors.Wrapf(err, errors.ErrActionExecute,
			"failed to execute operations")
	}

	e.logger.Info().Msg("All operations executed successfully")
	return results, nil
}

// executeDryRun handles dry run mode
func (e *SynthfsExecutor) executeDryRun(ops []types.Operation) []types.OperationResult {
	e.logger.Info().Msg("Dry run mode - operations would be executed:")
	results := make([]types.OperationResult, len(ops))

	for i, op := range ops {
		startTime := time.Now()
		result := types.OperationResult{
			Operation: &ops[i],
			StartTime: startTime,
			EndTime:   startTime, // Immediate for dry run
		}

		if op.Status == types.StatusReady {
			e.logOperation(op)
			result.Status = types.StatusReady
		} else {
			result.Status = op.Status
		}

		results[i] = result
	}

	return results
}

// convertToSynthfsOp converts a dodot operation to a synthfs operation
func (e *SynthfsExecutor) convertToSynthfsOp(sfs *synthfs.SynthFS, op types.Operation) (synthfs.Operation, error) {
	// Handle force mode for symlinks by pre-deleting existing files
	if e.force && op.Type == types.OperationCreateSymlink {
		if _, err := os.Lstat(op.Target); err == nil {
			e.logger.Debug().
				Str("target", op.Target).
				Msg("Force mode: will overwrite existing file")
		}
	}

	switch op.Type {
	case types.OperationCreateDir:
		return e.createDirOp(sfs, op)
	case types.OperationWriteFile:
		return e.createWriteFileOp(sfs, op)
	case types.OperationCreateSymlink:
		return e.createSymlinkOp(sfs, op)
	case types.OperationCopyFile:
		return e.createCopyOp(sfs, op)
	case types.OperationDeleteFile:
		return e.createDeleteOp(sfs, op)
	case types.OperationBackupFile:
		return e.createBackupOp(sfs, op)
	case types.OperationExecute:
		return e.createShellCommandOp(sfs, op)
	case types.OperationReadFile, types.OperationChecksum:
		// These are not actual file operations
		e.logger.Debug().
			Str("type", string(op.Type)).
			Msg("Skipping non-mutating operation")
		return nil, nil
	default:
		return nil, errors.Newf(errors.ErrActionInvalid,
			"unsupported operation type: %s", op.Type)
	}
}

// createDirOp creates a directory creation operation
func (e *SynthfsExecutor) createDirOp(sfs *synthfs.SynthFS, op types.Operation) (synthfs.Operation, error) {
	if op.Target == "" {
		return nil, errors.New(errors.ErrInvalidInput,
			"create directory operation requires target")
	}

	// Ensure we're only creating directories in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return nil, err
	}

	mode := e.config.FilePermissions.Directory
	if op.Mode != nil {
		mode = os.FileMode(*op.Mode)
	}

	e.logger.Debug().
		Str("target", op.Target).
		Str("mode", mode.String()).
		Msg("Creating directory operation")

	// Generate a unique ID based on the operation
	id := fmt.Sprintf("createdir_%s_%d", filepath.Base(op.Target), time.Now().UnixNano())
	return sfs.CreateDirWithID(id, op.Target, mode), nil
}

// createWriteFileOp creates a write file operation
func (e *SynthfsExecutor) createWriteFileOp(sfs *synthfs.SynthFS, op types.Operation) (synthfs.Operation, error) {
	if op.Target == "" {
		return nil, errors.New(errors.ErrInvalidInput,
			"write file operation requires target")
	}

	// Ensure we're only writing files in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return nil, err
	}

	mode := e.config.FilePermissions.File
	if op.Mode != nil {
		mode = os.FileMode(*op.Mode)
	}

	e.logger.Debug().
		Str("target", op.Target).
		Str("mode", mode.String()).
		Int("contentLen", len(op.Content)).
		Msg("Creating write file operation")

	id := fmt.Sprintf("writefile_%s_%d", filepath.Base(op.Target), time.Now().UnixNano())
	return sfs.CreateFileWithID(id, op.Target, []byte(op.Content), mode), nil
}

// createSymlinkOp creates a symlink operation
func (e *SynthfsExecutor) createSymlinkOp(sfs *synthfs.SynthFS, op types.Operation) (synthfs.Operation, error) {
	if op.Source == "" || op.Target == "" {
		return nil, errors.New(errors.ErrInvalidInput,
			"symlink operation requires source and target")
	}

	// For issue #71, handle symlinks in home directory with special validation
	if err := e.validateSymlinkPath(op.Target, op.Source); err != nil {
		return nil, err
	}

	e.logger.Info().
		Str("source", op.Source).
		Str("target", op.Target).
		Msg("Creating symlink operation")

	// If force mode, create a delete operation first
	if e.force {
		if _, err := os.Lstat(op.Target); err == nil {
			// Target exists, need to delete it first
			// We'll handle this by creating a custom operation that deletes then creates
			id := fmt.Sprintf("symlink_%s_%d", filepath.Base(op.Target), time.Now().UnixNano())
			return sfs.CustomOperationWithID(id, func(ctx context.Context, fs filesystem.FileSystem) error {
				// Delete existing file/symlink
				if err := fs.Remove(op.Target); err != nil && !os.IsNotExist(err) {
					return err
				}
				// Create new symlink
				return fs.Symlink(op.Source, op.Target)
			}), nil
		}
	}

	id := fmt.Sprintf("symlink_%s_%d", filepath.Base(op.Target), time.Now().UnixNano())
	return sfs.CreateSymlinkWithID(id, op.Source, op.Target), nil
}

// createCopyOp creates a copy operation
func (e *SynthfsExecutor) createCopyOp(sfs *synthfs.SynthFS, op types.Operation) (synthfs.Operation, error) {
	if op.Source == "" || op.Target == "" {
		return nil, errors.New(errors.ErrInvalidInput,
			"copy file operation requires source and target")
	}

	// Ensure we're only copying to safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return nil, err
	}

	e.logger.Debug().
		Str("source", op.Source).
		Str("target", op.Target).
		Msg("Creating copy operation")

	id := fmt.Sprintf("copy_%s_%d", filepath.Base(op.Target), time.Now().UnixNano())
	return sfs.CopyWithID(id, op.Source, op.Target), nil
}

// createDeleteOp creates a delete operation
func (e *SynthfsExecutor) createDeleteOp(sfs *synthfs.SynthFS, op types.Operation) (synthfs.Operation, error) {
	if op.Target == "" {
		return nil, errors.New(errors.ErrInvalidInput,
			"delete file operation requires target")
	}

	// Ensure we're only deleting files in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return nil, err
	}

	e.logger.Debug().
		Str("target", op.Target).
		Msg("Creating delete operation")

	id := fmt.Sprintf("delete_%s_%d", filepath.Base(op.Target), time.Now().UnixNano())
	return sfs.DeleteWithID(id, op.Target), nil
}

// createBackupOp creates a backup (copy) operation
func (e *SynthfsExecutor) createBackupOp(sfs *synthfs.SynthFS, op types.Operation) (synthfs.Operation, error) {
	if op.Source == "" || op.Target == "" {
		return nil, errors.New(errors.ErrInvalidInput,
			"backup file operation requires source and target")
	}

	// Ensure we're only creating backups in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return nil, err
	}

	e.logger.Debug().
		Str("source", op.Source).
		Str("target", op.Target).
		Msg("Creating backup (copy) operation")

	id := fmt.Sprintf("backup_%s_%d", filepath.Base(op.Target), time.Now().UnixNano())
	return sfs.CopyWithID(id, op.Source, op.Target), nil
}

// createShellCommandOp creates a shell command operation
func (e *SynthfsExecutor) createShellCommandOp(sfs *synthfs.SynthFS, op types.Operation) (synthfs.Operation, error) {
	if op.Command == "" {
		return nil, errors.New(errors.ErrInvalidInput,
			"execute operation requires command")
	}

	// Construct the full command
	fullCommand := op.Command
	if len(op.Args) > 0 {
		// Properly quote arguments that contain spaces
		quotedArgs := make([]string, len(op.Args))
		for i, arg := range op.Args {
			if strings.Contains(arg, " ") {
				quotedArgs[i] = fmt.Sprintf("%q", arg)
			} else {
				quotedArgs[i] = arg
			}
		}
		fullCommand = fmt.Sprintf("%s %s", op.Command, strings.Join(quotedArgs, " "))
	}

	// Create shell command options
	var options []synthfs.ShellCommandOption

	// Set working directory if provided
	if op.WorkingDir != "" {
		options = append(options, synthfs.WithWorkDir(op.WorkingDir))
	}

	// Set environment variables if provided
	if len(op.EnvironmentVars) > 0 {
		options = append(options, synthfs.WithEnv(op.EnvironmentVars))
	}

	// Always capture output for logging and result tracking
	options = append(options, synthfs.WithCaptureOutput())

	// Set timeout (default was 30 seconds in CommandExecutor)
	options = append(options, synthfs.WithTimeout(30*time.Second))

	e.logger.Info().
		Str("command", fullCommand).
		Str("workingDir", op.WorkingDir).
		Str("description", op.Description).
		Msg("Creating shell command operation")

	id := fmt.Sprintf("exec_%s_%d", strings.Fields(op.Command)[0], time.Now().UnixNano())
	return sfs.ShellCommandWithID(id, fullCommand, options...), nil
}

// convertResults converts synthfs results to dodot results
func (e *SynthfsExecutor) convertResults(result *synthfs.Result, dodotOpMap map[synthfs.OperationID]*types.Operation) []types.OperationResult {
	if result == nil {
		return []types.OperationResult{}
	}

	results := []types.OperationResult{}
	operations := result.GetOperations()

	for _, opResult := range operations {
		if synthfsResult, ok := opResult.(synthfs.OperationResult); ok {
			dodotOp, exists := dodotOpMap[synthfsResult.OperationID]
			if !exists {
				e.logger.Warn().
					Str("operationID", string(synthfsResult.OperationID)).
					Msg("Could not find dodot operation for synthfs result")
				continue
			}

			// Convert status
			var status types.OperationStatus
			switch synthfsResult.Status {
			case synthfs.StatusSuccess:
				status = types.StatusReady
			case synthfs.StatusFailure:
				status = types.StatusError
			case synthfs.StatusValidation:
				status = types.StatusError
			default:
				status = types.StatusError
			}

			// Extract output if available
			var output string
			if synthfsResult.Operation != nil {
				desc := synthfsResult.Operation.Describe()
				if desc.Details != nil {
					if stdout, ok := desc.Details["stdout"].(string); ok {
						output = stdout
					}
					if stderr, ok := desc.Details["stderr"].(string); ok && stderr != "" {
						if output != "" {
							output += "\n[stderr]\n" + stderr
						} else {
							output = "[stderr]\n" + stderr
						}
					}
				}
			}

			// Calculate times from duration
			// Note: synthfs doesn't provide exact start/end times, so we approximate
			endTime := time.Now()
			startTime := endTime.Add(-synthfsResult.Duration)

			results = append(results, types.OperationResult{
				Operation: dodotOp,
				Status:    status,
				Error:     synthfsResult.Error,
				StartTime: startTime,
				EndTime:   endTime,
				Output:    output,
			})
		}
	}

	return results
}

// validateSafePath ensures the path is within dodot-controlled directories
// For issue #70, we only allow operations in DataHome directories
func (e *SynthfsExecutor) validateSafePath(path string) error {
	if e.paths == nil {
		return errors.New(errors.ErrInternal,
			"paths not initialized")
	}

	// Normalize the path
	normalizedPath, err := filepath.Abs(path)
	if err != nil {
		return errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to normalize path: %s", path)
	}

	// Also try to resolve symlinks in the path (for macOS /var -> /private/var)
	resolvedPath := normalizedPath
	// Only resolve if parent directory exists
	if parentDir := filepath.Dir(normalizedPath); parentDir != "" {
		if resolvedParent, err := filepath.EvalSymlinks(parentDir); err == nil {
			resolvedPath = filepath.Join(resolvedParent, filepath.Base(normalizedPath))
		}
	}

	// Check if the path is within any of the safe directories
	safeDirectories := []string{
		e.paths.DotfilesRoot(), // Allow operations in dotfiles root for init/fill
		e.paths.DataDir(),
		e.paths.ConfigDir(),
		e.paths.CacheDir(),
		e.paths.StateDir(),
		// All subdirectories under DataDir are safe
		e.paths.DeployedDir(),
		e.paths.BackupsDir(),
		e.paths.HomebrewDir(),
		e.paths.InstallDir(),
		e.paths.ShellDir(),
		e.paths.TemplatesDir(),
	}

	for _, safeDir := range safeDirectories {
		// Resolve symlinks in safe directory path for comparison
		resolvedSafeDir := safeDir
		if resolved, err := filepath.EvalSymlinks(safeDir); err == nil {
			resolvedSafeDir = resolved
		}

		if isPathWithin(normalizedPath, safeDir) || isPathWithin(normalizedPath, resolvedSafeDir) ||
			isPathWithin(resolvedPath, safeDir) || isPathWithin(resolvedPath, resolvedSafeDir) {
			e.logger.Debug().
				Str("path", normalizedPath).
				Str("safeDir", safeDir).
				Msg("Path validated as safe")
			return nil
		}
	}

	// Check if home operations are allowed and path is in home directory
	// Note: For symlinks, validateSymlinkPath handles additional checks like protected files
	if e.allowHomeSymlinks {
		homeDir, err := paths.GetHomeDirectory()
		if err == nil && isPathWithin(normalizedPath, homeDir) {
			e.logger.Debug().
				Str("path", normalizedPath).
				Msg("Path validated as safe (home directory with allowHomeSymlinks)")
			return nil
		}
	}

	// For issue #71, we'll handle symlinks in user home directory
	// For now, reject operations outside safe directories
	return errors.Newf(errors.ErrPermission,
		"operation target is outside dodot-controlled directories: %s", path)
}

// isPathWithin checks if a path is within a parent directory
func isPathWithin(path, parent string) bool {
	// Normalize both paths
	path = filepath.Clean(path)
	parent = filepath.Clean(parent)

	// Check if path starts with parent
	rel, err := filepath.Rel(parent, path)
	if err != nil {
		return false
	}

	// If relative path starts with "..", it's outside parent
	return !strings.HasPrefix(rel, "..") && !strings.HasPrefix(rel, "/")
}

// validateSymlinkPath validates symlink creation with special handling for home directory
func (e *SynthfsExecutor) validateSymlinkPath(target, source string) error {
	// Normalize paths first
	normalizedTarget, err := filepath.Abs(target)
	if err != nil {
		return errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to normalize target path: %s", target)
	}

	// Check if target is in home directory
	homeDir, err := paths.GetHomeDirectory()
	if err != nil {
		return errors.Wrap(err, errors.ErrFileAccess,
			"failed to get home directory for validation")
	}

	isInHome := isPathWithin(normalizedTarget, homeDir)

	// First check if target is in standard safe directories (but not if it's in home)
	if !isInHome {
		err := e.validateSafePath(target)
		if err == nil {
			// Target is in a safe directory, allow it
			return nil
		}
		e.logger.Debug().
			Str("target", target).
			Str("normalizedTarget", normalizedTarget).
			Err(err).
			Msg("Target not in safe directories")

		// If not in safe directory and not home, check if home symlinks are allowed
		if !e.allowHomeSymlinks {
			return errors.Newf(errors.ErrPermission,
				"symlink target is outside dodot-controlled directories: %s", target)
		}
	}

	// If we get here, target is either in home or home symlinks are allowed
	if !e.allowHomeSymlinks && isInHome {
		return errors.Newf(errors.ErrPermission,
			"symlink target is outside dodot-controlled directories: %s", target)
	}

	// Home symlinks are allowed, perform additional safety checks

	// First, validate the source is from dotfiles
	dotfilesRoot := e.paths.DotfilesRoot()
	normalizedSource, err := filepath.Abs(source)
	if err != nil {
		return errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to normalize source path: %s", source)
	}

	// Source must be from either dotfiles or deployed directories
	deployedDir := e.paths.DeployedDir()
	if !isPathWithin(normalizedSource, dotfilesRoot) && !isPathWithin(normalizedSource, deployedDir) {
		return errors.Newf(errors.ErrPermission,
			"symlink source must be from dotfiles or deployed directory: %s", source)
	}

	// Target path is already normalized above (normalizedTarget)
	// Home directory is already retrieved above (homeDir)

	// For macOS, handle /var -> /private/var resolution
	// Since the target may not exist yet, we need to check parent directories
	parentDir := filepath.Dir(normalizedTarget)
	if evalParent, err := filepath.EvalSymlinks(parentDir); err == nil {
		normalizedTarget = filepath.Join(evalParent, filepath.Base(normalizedTarget))
	}

	// Normalize home directory too for consistent comparison
	normalizedHome := homeDir
	if evalHome, err := filepath.EvalSymlinks(homeDir); err == nil {
		normalizedHome = evalHome
	} else {
		// If EvalSymlinks fails (e.g., directory doesn't exist),
		// try to at least make paths consistent by resolving the parent
		if homeParent := filepath.Dir(normalizedHome); homeParent != "" {
			if evalHomeParent, err := filepath.EvalSymlinks(homeParent); err == nil {
				normalizedHome = filepath.Join(evalHomeParent, filepath.Base(normalizedHome))
			}
		}
	}

	e.logger.Debug().
		Str("target", target).
		Str("normalizedTarget", normalizedTarget).
		Str("homeDir", homeDir).
		Str("normalizedHome", normalizedHome).
		Msg("Checking if target is in home directory")

	// Check if target is in home directory
	if !isPathWithin(normalizedTarget, normalizedHome) {
		// Target must be in home directory when allowHomeSymlinks is true
		return errors.Newf(errors.ErrPermission,
			"symlink target must be in home directory when using home symlinks: %s", target)
	}

	// Target is in home directory, perform additional safety checks
	// Check for dangerous target locations
	if err := e.validateNotSystemFile(normalizedTarget); err != nil {
		return err
	}

	e.logger.Debug().
		Str("source", normalizedSource).
		Str("target", normalizedTarget).
		Bool("homeSymlinksAllowed", e.allowHomeSymlinks).
		Msg("Symlink path validated for home directory")

	return nil
}

// validateNotSystemFile ensures we're not overwriting critical system files
func (e *SynthfsExecutor) validateNotSystemFile(path string) error {
	e.logger.Debug().
		Str("path", path).
		Msg("Checking if path is a protected system file")

	homeDir, _ := paths.GetHomeDirectory()
	// Normalize home directory for consistent comparison
	if evalHome, err := filepath.EvalSymlinks(homeDir); err == nil {
		homeDir = evalHome
	}

	relPath, err := filepath.Rel(homeDir, path)
	if err != nil {
		// Not in home directory, can't check
		return nil
	}

	// Check if the relative path matches any protected path
	// First check exact match
	if e.config.Security.ProtectedPaths[relPath] {
		e.logger.Warn().
			Str("path", path).
			Str("relPath", relPath).
			Msg("Blocking symlink to protected file")
		return errors.Newf(errors.ErrPermission,
			"cannot create symlink for protected file: %s", relPath)
	}

	// Then check if the path is within a protected directory
	for protectedPath := range e.config.Security.ProtectedPaths {
		if strings.HasPrefix(relPath, protectedPath+"/") {
			e.logger.Warn().
				Str("path", path).
				Str("relPath", relPath).
				Str("protected", protectedPath).
				Msg("Blocking symlink to protected file")
			return errors.Newf(errors.ErrPermission,
				"cannot create symlink for protected file: %s", relPath)
		}
	}

	// Warn about existing files that will be replaced
	if info, err := os.Stat(path); err == nil && !e.dryRun {
		if info.Mode()&os.ModeSymlink == 0 {
			// It's a real file, not a symlink
			e.logger.Warn().
				Str("path", path).
				Bool("isDir", info.IsDir()).
				Bool("backupEnabled", e.backupExisting).
				Msg("Existing file will be replaced by symlink")
		}
	}

	return nil
}

// logOperation logs details about an operation
func (e *SynthfsExecutor) logOperation(op types.Operation) {
	logger := e.logger.With().
		Str("type", string(op.Type)).
		Str("description", op.Description).
		Logger()

	switch op.Type {
	case types.OperationCreateSymlink:
		logger.Info().
			Str("source", op.Source).
			Str("target", op.Target).
			Msg("Would create symlink")
	case types.OperationCreateDir:
		logger.Info().
			Str("target", op.Target).
			Msg("Would create directory")
	case types.OperationWriteFile:
		logger.Info().
			Str("target", op.Target).
			Int("contentLen", len(op.Content)).
			Msg("Would write file")
	case types.OperationCopyFile:
		logger.Info().
			Str("source", op.Source).
			Str("target", op.Target).
			Msg("Would copy file")
	case types.OperationDeleteFile:
		logger.Info().
			Str("target", op.Target).
			Msg("Would delete file")
	case types.OperationExecute:
		logger.Info().
			Str("command", op.Command).
			Strs("args", op.Args).
			Str("workingDir", op.WorkingDir).
			Msg("Would execute command")
	default:
		logger.Info().Msg("Would execute operation")
	}
}
