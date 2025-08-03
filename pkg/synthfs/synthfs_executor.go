package synthfs

import (
	"context"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs"
	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
	"github.com/arthur-debert/synthfs/pkg/synthfs/operations"
	"github.com/rs/zerolog"
)

// SynthfsExecutor executes dodot operations using synthfs
type SynthfsExecutor struct {
	logger            zerolog.Logger
	dryRun            bool
	force             bool
	filesystem        synthfs.FileSystem
	paths             *paths.Paths
	allowHomeSymlinks bool
	backupExisting    bool
}

// NewSynthfsExecutor creates a new synthfs-based executor
func NewSynthfsExecutor(dryRun bool) *SynthfsExecutor {
	// Initialize paths with empty string to use defaults
	p, _ := paths.New("")
	return &SynthfsExecutor{
		logger:            logging.GetLogger("core.synthfs"),
		dryRun:            dryRun,
		filesystem:        filesystem.NewOSFileSystem("/"), // Use root filesystem
		paths:             p,
		allowHomeSymlinks: false, // Default to safe mode
		backupExisting:    true,  // Default to backing up existing files
	}
}

// NewSynthfsExecutorWithPaths creates a new synthfs-based executor with custom paths
func NewSynthfsExecutorWithPaths(dryRun bool, p *paths.Paths) *SynthfsExecutor {
	return &SynthfsExecutor{
		logger:            logging.GetLogger("core.synthfs"),
		dryRun:            dryRun,
		filesystem:        filesystem.NewOSFileSystem("/"), // Use root filesystem
		paths:             p,
		allowHomeSymlinks: false,
		backupExisting:    true,
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

	// Before converting operations, check if we need to clean up existing files for force mode
	// This is needed because synthfs validation will fail on existing symlinks
	if e.force {
		for _, op := range ops {
			if op.Status == types.StatusReady && op.Type == types.OperationCreateSymlink {
				// Check if target exists and if so, remove it to allow overwrite
				if _, err := os.Lstat(op.Target); err == nil {
					e.logger.Debug().
						Str("target", op.Target).
						Msg("Removing existing file to allow overwrite in force mode")
					if err := os.Remove(op.Target); err != nil {
						e.logger.Warn().
							Err(err).
							Str("target", op.Target).
							Msg("Failed to remove existing file in force mode")
					}
				}
			}
		}
	}

	// Convert dodot operations to synthfs operations
	synthOps := make([]synthfs.Operation, 0, len(ops))
	for _, op := range ops {
		if op.Status != types.StatusReady {
			e.logger.Debug().
				Str("type", string(op.Type)).
				Str("target", op.Target).
				Str("status", string(op.Status)).
				Msg("Skipping operation with non-ready status")
			continue
		}

		synthOp, err := e.convertToSynthfsOperation(op)
		if err != nil {
			return errors.Wrapf(err, errors.ErrActionExecute,
				"failed to convert operation: %s", op.Description)
		}
		if synthOp != nil {
			synthOps = append(synthOps, synthOp)
		}
	}

	if len(synthOps) == 0 {
		e.logger.Info().Msg("No operations to execute")
		return nil
	}

	// Create a synthfs pipeline with the operations
	pipeline := synthfs.NewMemPipeline()
	for _, op := range synthOps {
		if err := pipeline.Add(op); err != nil {
			return errors.Wrapf(err, errors.ErrActionExecute,
				"failed to add operation to pipeline")
		}
	}

	// Execute the pipeline
	ctx := context.Background()
	executor := synthfs.NewExecutor()

	e.logger.Info().Int("operationCount", len(synthOps)).Msg("Executing operations")

	result := executor.Run(ctx, pipeline, e.filesystem)
	if result.GetError() != nil {
		e.logger.Error().Err(result.GetError()).Msg("Pipeline execution failed")
		return errors.Wrapf(result.GetError(), errors.ErrActionExecute,
			"failed to execute operations")
	}

	e.logger.Info().Msg("All operations executed successfully")
	return nil
}

// convertToSynthfsOperation converts a dodot operation to a synthfs operation
func (e *SynthfsExecutor) convertToSynthfsOperation(op types.Operation) (synthfs.Operation, error) {
	switch op.Type {
	case types.OperationCreateDir:
		return e.convertCreateDir(op)
	case types.OperationWriteFile:
		return e.convertWriteFile(op)
	case types.OperationCreateSymlink:
		return e.convertCreateSymlink(op)
	case types.OperationCopyFile:
		return e.convertCopyFile(op)
	case types.OperationDeleteFile:
		return e.convertDeleteFile(op)
	case types.OperationBackupFile:
		return e.convertBackupFile(op)
	case types.OperationReadFile, types.OperationChecksum:
		// These are not actual file operations
		e.logger.Debug().
			Str("type", string(op.Type)).
			Msg("Skipping non-mutating operation")
		return nil, nil
	case types.OperationExecute:
		// Execute operations need special handling outside of synthfs
		// For now, skip them in synthfs and handle them separately
		e.logger.Debug().
			Str("type", string(op.Type)).
			Str("command", op.Command).
			Msg("Skipping execute operation in synthfs (needs separate handling)")
		return nil, nil
	default:
		return nil, errors.Newf(errors.ErrActionInvalid,
			"unsupported operation type: %s", op.Type)
	}
}

// convertCreateDir converts a create directory operation
func (e *SynthfsExecutor) convertCreateDir(op types.Operation) (synthfs.Operation, error) {
	if op.Target == "" {
		return nil, errors.New(errors.ErrInvalidInput,
			"create directory operation requires target")
	}

	// Ensure we're only creating directories in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return nil, err
	}

	mode := os.FileMode(0755)
	if op.Mode != nil {
		mode = os.FileMode(*op.Mode)
	}

	e.logger.Debug().
		Str("target", op.Target).
		Str("mode", mode.String()).
		Msg("Creating directory operation")

	// Create the synthfs operation
	// Convert absolute path to relative for synthfs
	relPath, err := filepath.Rel("/", op.Target)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to convert path: %s", op.Target)
	}

	opID := core.OperationID(fmt.Sprintf("create-dir-%s", op.Target))
	createOp := operations.NewCreateDirectoryOperation(opID, relPath)

	// Set the mode via item
	createOp.SetItem(&directoryItem{
		path: relPath,
		mode: mode,
	})

	return synthfs.NewOperationsPackageAdapter(createOp), nil
}

// convertWriteFile converts a write file operation
func (e *SynthfsExecutor) convertWriteFile(op types.Operation) (synthfs.Operation, error) {
	if op.Target == "" {
		return nil, errors.New(errors.ErrInvalidInput,
			"write file operation requires target")
	}

	// Ensure we're only writing files in safe locations
	if err := e.validateSafePath(op.Target); err != nil {
		return nil, err
	}

	mode := os.FileMode(0644)
	if op.Mode != nil {
		mode = os.FileMode(*op.Mode)
	}

	e.logger.Debug().
		Str("target", op.Target).
		Str("mode", mode.String()).
		Int("contentLen", len(op.Content)).
		Msg("Creating write file operation")

	// Create the synthfs operation
	// Convert absolute path to relative for synthfs
	relPath, err := filepath.Rel("/", op.Target)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to convert path: %s", op.Target)
	}

	opID := core.OperationID(fmt.Sprintf("write-file-%s", op.Target))
	createOp := operations.NewCreateFileOperation(opID, relPath)

	// Set the content via item
	createOp.SetItem(&fileItem{
		path:    relPath,
		content: []byte(op.Content),
		mode:    mode,
	})

	return synthfs.NewOperationsPackageAdapter(createOp), nil
}

// convertCreateSymlink converts a create symlink operation
func (e *SynthfsExecutor) convertCreateSymlink(op types.Operation) (synthfs.Operation, error) {
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

	// Create the synthfs operation
	// Convert absolute path to relative for synthfs
	relPath, err := filepath.Rel("/", op.Target)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to convert path: %s", op.Target)
	}

	// Convert source path to relative as well
	relSource, err := filepath.Rel("/", op.Source)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to convert source path: %s", op.Source)
	}

	opID := core.OperationID(fmt.Sprintf("symlink-%s", op.Target))
	symlinkOp := operations.NewCreateSymlinkOperation(opID, relPath)

	// Set the target via description detail
	symlinkOp.SetDescriptionDetail("target", relSource)

	// Set a minimal item (synthfs requires it)
	symlinkOp.SetItem(&symlinkItem{
		path:   relPath,
		target: relSource,
	})

	return synthfs.NewOperationsPackageAdapter(symlinkOp), nil
}

// convertCopyFile converts a copy file operation
func (e *SynthfsExecutor) convertCopyFile(op types.Operation) (synthfs.Operation, error) {
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
		Msg("Creating copy file operation")

	// Create the synthfs operation
	// Convert paths to relative for synthfs
	relSource, err := filepath.Rel("/", op.Source)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to convert source path: %s", op.Source)
	}
	relTarget, err := filepath.Rel("/", op.Target)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to convert target path: %s", op.Target)
	}

	opID := core.OperationID(fmt.Sprintf("copy-%s-to-%s", filepath.Base(op.Source), op.Target))
	copyOp := operations.NewCopyOperation(opID, relTarget)

	// Set source and destination paths
	copyOp.SetPaths(relSource, relTarget)

	return synthfs.NewOperationsPackageAdapter(copyOp), nil
}

// convertDeleteFile converts a delete file operation
func (e *SynthfsExecutor) convertDeleteFile(op types.Operation) (synthfs.Operation, error) {
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
		Msg("Creating delete file operation")

	// Create the synthfs operation
	// Convert absolute path to relative for synthfs
	relPath, err := filepath.Rel("/", op.Target)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to convert path: %s", op.Target)
	}

	opID := core.OperationID(fmt.Sprintf("delete-%s", op.Target))
	deleteOp := operations.NewDeleteOperation(opID, relPath)

	return synthfs.NewOperationsPackageAdapter(deleteOp), nil
}

// convertBackupFile converts a backup file operation
func (e *SynthfsExecutor) convertBackupFile(op types.Operation) (synthfs.Operation, error) {
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

	// Backup is essentially a copy operation
	// Convert paths to relative for synthfs
	relSource, err := filepath.Rel("/", op.Source)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to convert source path: %s", op.Source)
	}
	relTarget, err := filepath.Rel("/", op.Target)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInvalidInput,
			"failed to convert target path: %s", op.Target)
	}

	opID := core.OperationID(fmt.Sprintf("backup-%s", filepath.Base(op.Source)))
	copyOp := operations.NewCopyOperation(opID, relTarget)

	// Set source and destination paths
	copyOp.SetPaths(relSource, relTarget)

	return synthfs.NewOperationsPackageAdapter(copyOp), nil
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
		e.paths.BrewfileDir(),
		e.paths.InstallDir(),
		e.paths.ShellDir(),
		e.paths.TemplatesDir(),
	}

	for _, safeDir := range safeDirectories {
		if isPathWithin(normalizedPath, safeDir) {
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

	// List of protected files/directories that should never be symlinked
	protectedPaths := []string{
		".ssh/authorized_keys",
		".ssh/id_rsa",
		".ssh/id_ed25519",
		".gnupg",
		".password-store",
		".config/gh/hosts.yml", // GitHub CLI auth
		".aws/credentials",
		".kube/config",
		".docker/config.json",
	}

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
	for _, protected := range protectedPaths {
		// Check exact match or if the path is within a protected directory
		if relPath == protected || strings.HasPrefix(relPath, protected+"/") {
			e.logger.Warn().
				Str("path", path).
				Str("relPath", relPath).
				Str("protected", protected).
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

// Item types for synthfs operations

// fileItem implements the interface needed for file operations
type fileItem struct {
	path    string
	content []byte
	mode    fs.FileMode
}

func (f *fileItem) Path() string       { return f.path }
func (f *fileItem) Type() string       { return "file" }
func (f *fileItem) Content() []byte    { return f.content }
func (f *fileItem) Mode() fs.FileMode  { return f.mode }
func (f *fileItem) IsDir() bool        { return false }
func (f *fileItem) ModTime() time.Time { return time.Now() }
func (f *fileItem) Size() int64        { return int64(len(f.content)) }

// directoryItem implements the interface needed for directory operations
type directoryItem struct {
	path string
	mode fs.FileMode
}

func (d *directoryItem) Path() string       { return d.path }
func (d *directoryItem) Type() string       { return "directory" }
func (d *directoryItem) Mode() fs.FileMode  { return d.mode }
func (d *directoryItem) IsDir() bool        { return true }
func (d *directoryItem) ModTime() time.Time { return time.Now() }
func (d *directoryItem) Size() int64        { return 0 }

// symlinkItem implements the interface needed for symlink operations
type symlinkItem struct {
	path   string
	target string
}

func (s *symlinkItem) Path() string   { return s.path }
func (s *symlinkItem) Type() string   { return "symlink" }
func (s *symlinkItem) Target() string { return s.target }
