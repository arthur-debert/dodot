package synthfs

import (
	"context"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
	"github.com/rs/zerolog"
)

// protectedPathsMap contains paths that should never be symlinked for security reasons
var protectedPathsMap = map[string]bool{
	".ssh/authorized_keys": true,
	".ssh/id_rsa":          true,
	".ssh/id_ed25519":      true,
	".gnupg":               true,
	".password-store":      true,
	".config/gh/hosts.yml": true, // GitHub CLI auth
	".aws/credentials":     true,
	".kube/config":         true,
	".docker/config.json":  true,
}

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
}

// NewSynthfsExecutor creates a new synthfs-based executor
func NewSynthfsExecutor(dryRun bool) *SynthfsExecutor {
	// Initialize paths with empty string to use defaults
	p, _ := paths.New("")
	// Use PathAwareFileSystem to handle absolute paths directly
	osfs := filesystem.NewOSFileSystem("/")
	pathAwareFS := synthfs.NewPathAwareFileSystem(osfs, "/").WithAbsolutePaths()

	return &SynthfsExecutor{
		logger:            logging.GetLogger("core.synthfs"),
		dryRun:            dryRun,
		filesystem:        pathAwareFS,
		paths:             p,
		allowHomeSymlinks: false, // Default to safe mode
		backupExisting:    true,  // Default to backing up existing files
		enableRollback:    true,  // Default to enabling rollback for safety
	}
}

// NewSynthfsExecutorWithPaths creates a new synthfs-based executor with custom paths
func NewSynthfsExecutorWithPaths(dryRun bool, p *paths.Paths) *SynthfsExecutor {
	// Use PathAwareFileSystem to handle absolute paths directly
	osfs := filesystem.NewOSFileSystem("/")
	pathAwareFS := synthfs.NewPathAwareFileSystem(osfs, "/").WithAbsolutePaths()

	return &SynthfsExecutor{
		logger:            logging.GetLogger("core.synthfs"),
		dryRun:            dryRun,
		filesystem:        pathAwareFS,
		paths:             p,
		allowHomeSymlinks: false,
		backupExisting:    true,
		enableRollback:    true,
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
func (e *SynthfsExecutor) ExecuteOperations(ops []types.Operation) error {
	if e.dryRun {
		e.logger.Info().Msg("Dry run mode - operations would be executed:")
		for _, op := range ops {
			if op.Status == types.StatusReady {
				e.logOperation(op)
			}
		}
		return nil
	}

	// Create a SimpleBatch for collecting operations
	batch := synthfs.NewSimpleBatch(e.filesystem)

	// Process each operation
	for _, op := range ops {
		if op.Status != types.StatusReady {
			e.logger.Debug().
				Str("type", string(op.Type)).
				Str("target", op.Target).
				Str("status", string(op.Status)).
				Msg("Skipping operation with non-ready status")
			continue
		}

		// Add the operation to the batch
		if err := e.addOperationToBatch(batch, op); err != nil {
			return errors.Wrapf(err, errors.ErrActionExecute,
				"failed to add operation: %s", op.Description)
		}
	}

	// Check if we have any operations to execute
	if len(batch.Operations()) == 0 {
		e.logger.Info().Msg("No operations to execute")
		return nil
	}

	// Execute the batch
	e.logger.Info().
		Int("operationCount", len(batch.Operations())).
		Bool("rollbackEnabled", e.enableRollback).
		Msg("Executing operations")

	ctx := context.Background()
	var err error

	if e.enableRollback {
		// Use ExecuteWithRollback for safer operations
		err = batch.WithContext(ctx).ExecuteWithRollback()
		if err != nil {
			e.logger.Error().
				Err(err).
				Msg("Batch execution failed, rollback was attempted")
		}
	} else {
		// Use regular Execute without rollback
		err = batch.WithContext(ctx).Execute()
		if err != nil {
			e.logger.Error().
				Err(err).
				Msg("Batch execution failed")
		}
	}

	if err != nil {
		return errors.Wrapf(err, errors.ErrActionExecute,
			"failed to execute operations")
	}

	e.logger.Info().Msg("All operations executed successfully")
	return nil
}

// addOperationToBatch adds a dodot operation to the synthfs batch
func (e *SynthfsExecutor) addOperationToBatch(batch *synthfs.SimpleBatch, op types.Operation) error {
	// Handle force mode for symlinks by pre-deleting existing files
	if e.force && op.Type == types.OperationCreateSymlink {
		if _, err := os.Lstat(op.Target); err == nil {
			e.logger.Debug().
				Str("target", op.Target).
				Msg("Adding delete operation for existing file in force mode")
			// PathAwareFileSystem handles absolute paths directly
			batch.Delete(op.Target)
		}
	}

	switch op.Type {
	case types.OperationCreateDir:
		return e.addCreateDir(batch, op)
	case types.OperationWriteFile:
		return e.addWriteFile(batch, op)
	case types.OperationCreateSymlink:
		return e.addCreateSymlink(batch, op)
	case types.OperationCopyFile:
		return e.addCopyFile(batch, op)
	case types.OperationDeleteFile:
		return e.addDeleteFile(batch, op)
	case types.OperationBackupFile:
		return e.addBackupFile(batch, op)
	case types.OperationReadFile, types.OperationChecksum:
		// These are not actual file operations
		e.logger.Debug().
			Str("type", string(op.Type)).
			Msg("Skipping non-mutating operation")
		return nil
	case types.OperationExecute:
		// Execute operations need special handling outside of synthfs
		// For now, skip them in synthfs and handle them separately
		e.logger.Debug().
			Str("type", string(op.Type)).
			Str("command", op.Command).
			Msg("Skipping execute operation in synthfs (needs separate handling)")
		return nil
	default:
		return errors.Newf(errors.ErrActionInvalid,
			"unsupported operation type: %s", op.Type)
	}
}

// addCreateDir adds a create directory operation to the batch
func (e *SynthfsExecutor) addCreateDir(batch *synthfs.SimpleBatch, op types.Operation) error {
	if op.Target == "" {
		return errors.New(errors.ErrInvalidInput,
			"create directory operation requires target")
	}

	// Ensure we're only creating directories in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return err
	}

	mode := os.FileMode(0755)
	if op.Mode != nil {
		mode = os.FileMode(*op.Mode)
	}

	e.logger.Debug().
		Str("target", op.Target).
		Str("mode", mode.String()).
		Msg("Adding create directory operation")

	// PathAwareFileSystem handles absolute paths directly
	batch.CreateDir(op.Target, mode)
	return nil
}

// addWriteFile adds a write file operation to the batch
func (e *SynthfsExecutor) addWriteFile(batch *synthfs.SimpleBatch, op types.Operation) error {
	if op.Target == "" {
		return errors.New(errors.ErrInvalidInput,
			"write file operation requires target")
	}

	// Ensure we're only writing files in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return err
	}

	mode := os.FileMode(0644)
	if op.Mode != nil {
		mode = os.FileMode(*op.Mode)
	}

	e.logger.Debug().
		Str("target", op.Target).
		Str("mode", mode.String()).
		Int("contentLen", len(op.Content)).
		Msg("Adding write file operation")

	// PathAwareFileSystem handles absolute paths directly
	batch.WriteFile(op.Target, []byte(op.Content), mode)
	return nil
}

// addCreateSymlink adds a create symlink operation to the batch
func (e *SynthfsExecutor) addCreateSymlink(batch *synthfs.SimpleBatch, op types.Operation) error {
	if op.Source == "" || op.Target == "" {
		return errors.New(errors.ErrInvalidInput,
			"symlink operation requires source and target")
	}

	// For issue #71, handle symlinks in home directory with special validation
	if err := e.validateSymlinkPath(op.Target, op.Source); err != nil {
		return err
	}

	e.logger.Info().
		Str("source", op.Source).
		Str("target", op.Target).
		Msg("Adding symlink operation")

	// PathAwareFileSystem handles absolute paths directly
	batch.CreateSymlink(op.Source, op.Target)
	return nil
}

// addCopyFile adds a copy file operation to the batch
func (e *SynthfsExecutor) addCopyFile(batch *synthfs.SimpleBatch, op types.Operation) error {
	if op.Source == "" || op.Target == "" {
		return errors.New(errors.ErrInvalidInput,
			"copy file operation requires source and target")
	}

	// Ensure we're only copying to safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return err
	}

	e.logger.Debug().
		Str("source", op.Source).
		Str("target", op.Target).
		Msg("Adding copy file operation")

	// PathAwareFileSystem handles absolute paths directly
	batch.Copy(op.Source, op.Target)
	return nil
}

// addDeleteFile adds a delete file operation to the batch
func (e *SynthfsExecutor) addDeleteFile(batch *synthfs.SimpleBatch, op types.Operation) error {
	if op.Target == "" {
		return errors.New(errors.ErrInvalidInput,
			"delete file operation requires target")
	}

	// Ensure we're only deleting files in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return err
	}

	e.logger.Debug().
		Str("target", op.Target).
		Msg("Adding delete file operation")

	// PathAwareFileSystem handles absolute paths directly
	batch.Delete(op.Target)
	return nil
}

// addBackupFile adds a backup file operation to the batch
func (e *SynthfsExecutor) addBackupFile(batch *synthfs.SimpleBatch, op types.Operation) error {
	if op.Source == "" || op.Target == "" {
		return errors.New(errors.ErrInvalidInput,
			"backup file operation requires source and target")
	}

	// Ensure we're only creating backups in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return err
	}

	e.logger.Debug().
		Str("source", op.Source).
		Str("target", op.Target).
		Msg("Adding backup (copy) operation")

	// PathAwareFileSystem handles absolute paths directly
	// Backup is essentially a copy operation
	batch.Copy(op.Source, op.Target)
	return nil
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
	if protectedPathsMap[relPath] {
		e.logger.Warn().
			Str("path", path).
			Str("relPath", relPath).
			Msg("Blocking symlink to protected file")
		return errors.Newf(errors.ErrPermission,
			"cannot create symlink for protected file: %s", relPath)
	}

	// Then check if the path is within a protected directory
	for protectedPath := range protectedPathsMap {
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
	default:
		logger.Info().Msg("Would execute operation")
	}
}
