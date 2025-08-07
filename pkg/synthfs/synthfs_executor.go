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
	// Skip non-mutating operations
	if op.Type == types.OperationReadFile || op.Type == types.OperationChecksum {
		e.logger.Debug().Str("type", string(op.Type)).Msg("Skipping non-mutating operation")
		return nil, nil
	}

	// Generate unique ID
	target := op.Target
	if target == "" && op.Command != "" {
		cmdParts := strings.Fields(op.Command)
		if len(cmdParts) > 0 {
			target = cmdParts[0]
		} else {
			target = "empty"
		}
	}
	id := fmt.Sprintf("%s_%s_%d", op.Type, filepath.Base(target), time.Now().UnixNano())

	// Note: Path validation is primarily done earlier in the pipeline during operation conversion
	// This provides better error context and "fail fast" behavior
	// However, for operations created directly (e.g., in tests), we still do basic validation
	if op.Type != types.OperationExecute && op.Target != "" {
		// Only do basic safety check for operations outside the normal pipeline
		if !e.allowHomeSymlinks && !isInSafeDirectory(op.Target, e.paths) {
			return nil, errors.Newf(errors.ErrPermission,
				"operation target is outside dodot-controlled directories: %s", op.Target)
		}
	}

	// Log the operation
	e.logger.Debug().
		Str("type", string(op.Type)).
		Str("target", op.Target).
		Str("description", op.Description).
		Msg("Converting operation")

	// Convert based on type
	switch op.Type {
	case types.OperationCreateDir:
		if op.Target == "" {
			return nil, errors.New(errors.ErrInvalidInput, "create directory operation requires target")
		}
		mode := e.config.FilePermissions.Directory
		if op.Mode != nil {
			mode = os.FileMode(*op.Mode)
		}
		return sfs.CreateDirWithID(id, op.Target, mode), nil

	case types.OperationWriteFile:
		if op.Target == "" {
			return nil, errors.New(errors.ErrInvalidInput, "write file operation requires target")
		}
		mode := e.config.FilePermissions.File
		if op.Mode != nil {
			mode = os.FileMode(*op.Mode)
		}
		return sfs.CreateFileWithID(id, op.Target, []byte(op.Content), mode), nil

	case types.OperationCreateSymlink:
		if op.Source == "" || op.Target == "" {
			return nil, errors.New(errors.ErrInvalidInput, "symlink operation requires source and target")
		}
		// Note: Symlink validation is primarily done earlier in the pipeline
		// For operations created directly, do basic validation
		if !e.allowHomeSymlinks && !isInSafeDirectory(op.Target, e.paths) {
			return nil, errors.Newf(errors.ErrPermission,
				"symlink target is outside dodot-controlled directories: %s", op.Target)
		}

		// Check for protected paths
		if e.allowHomeSymlinks && e.config != nil {
			if err := e.validateNotProtectedPath(op.Target); err != nil {
				return nil, err
			}
		}

		// Validate source is from dotfiles or deployed
		normalizedSource, err := filepath.Abs(op.Source)
		if err == nil {
			dotfilesRoot := e.paths.DotfilesRoot()
			deployedDir := e.paths.DeployedDir()
			if !isPathWithin(normalizedSource, dotfilesRoot) && !isPathWithin(normalizedSource, deployedDir) {
				return nil, errors.Newf(errors.ErrPermission,
					"symlink source must be from dotfiles or deployed directory: %s", op.Source)
			}
		}
		e.logger.Info().Str("source", op.Source).Str("target", op.Target).Msg("Creating symlink operation")

		// Handle force mode
		if e.force {
			if _, err := os.Lstat(op.Target); err == nil {
				// Create custom operation that deletes then creates
				return sfs.CustomOperationWithID(id, func(ctx context.Context, fs filesystem.FileSystem) error {
					if err := fs.Remove(op.Target); err != nil && !os.IsNotExist(err) {
						return err
					}
					return fs.Symlink(op.Source, op.Target)
				}), nil
			}
		}
		return sfs.CreateSymlinkWithID(id, op.Source, op.Target), nil

	case types.OperationCopyFile:
		if op.Source == "" || op.Target == "" {
			return nil, errors.New(errors.ErrInvalidInput, "copy file operation requires source and target")
		}
		return sfs.CopyWithID(id, op.Source, op.Target), nil

	case types.OperationDeleteFile:
		if op.Target == "" {
			return nil, errors.New(errors.ErrInvalidInput, "delete file operation requires target")
		}
		return sfs.DeleteWithID(id, op.Target), nil

	case types.OperationBackupFile:
		if op.Source == "" || op.Target == "" {
			return nil, errors.New(errors.ErrInvalidInput, "backup file operation requires source and target")
		}
		return sfs.CopyWithID(id, op.Source, op.Target), nil

	case types.OperationExecute:
		if op.Command == "" {
			return nil, errors.New(errors.ErrInvalidInput, "execute operation requires command")
		}

		// Construct full command
		fullCommand := op.Command
		if len(op.Args) > 0 {
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

		// Build options
		var options []synthfs.ShellCommandOption
		if op.WorkingDir != "" {
			options = append(options, synthfs.WithWorkDir(op.WorkingDir))
		}
		if len(op.EnvironmentVars) > 0 {
			options = append(options, synthfs.WithEnv(op.EnvironmentVars))
		}
		options = append(options, synthfs.WithCaptureOutput())
		options = append(options, synthfs.WithTimeout(30*time.Second))

		e.logger.Info().Str("command", fullCommand).Str("workingDir", op.WorkingDir).Msg("Creating shell command operation")
		return sfs.ShellCommandWithID(id, fullCommand, options...), nil

	default:
		return nil, errors.Newf(errors.ErrActionInvalid, "unsupported operation type: %s", op.Type)
	}
}

// convertResults converts synthfs results to dodot results
func (e *SynthfsExecutor) convertResults(result *synthfs.Result, dodotOpMap map[synthfs.OperationID]*types.Operation) []types.OperationResult {
	if result == nil {
		return []types.OperationResult{}
	}

	// Status mapping
	statusMap := map[synthfs.OperationStatus]types.OperationStatus{
		synthfs.StatusSuccess:    types.StatusReady,
		synthfs.StatusFailure:    types.StatusError,
		synthfs.StatusValidation: types.StatusError,
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

			// Map status
			status := statusMap[synthfsResult.Status]
			if status == "" {
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

// isInSafeDirectory checks if a path is within any of the safe directories
// This is a simplified check for operations created outside the normal pipeline
func isInSafeDirectory(path string, p *paths.Paths) bool {
	normalizedPath, err := filepath.Abs(path)
	if err != nil {
		return false
	}

	safeDirectories := []string{
		p.DotfilesRoot(),
		p.DataDir(),
		p.ConfigDir(),
		p.CacheDir(),
		p.StateDir(),
		p.DeployedDir(),
		p.BackupsDir(),
		p.HomebrewDir(),
		p.InstallDir(),
		p.ShellDir(),
		p.TemplatesDir(),
	}

	for _, safeDir := range safeDirectories {
		if isPathWithin(normalizedPath, safeDir) {
			return true
		}
	}

	return false
}

// isPathWithin checks if a path is within a parent directory
func isPathWithin(path, parent string) bool {
	path = filepath.Clean(path)
	parent = filepath.Clean(parent)

	rel, err := filepath.Rel(parent, path)
	if err != nil {
		return false
	}

	return !strings.HasPrefix(rel, "..") && !strings.HasPrefix(rel, "/")
}

// validateNotProtectedPath checks if a path is a protected system file
// This is a simplified version for operations created outside the normal pipeline
func (e *SynthfsExecutor) validateNotProtectedPath(path string) error {
	// In test environments, HOME might be set to a temp directory
	// Get it from environment first
	homeDir := os.Getenv("HOME")
	if homeDir == "" {
		var err error
		homeDir, err = paths.GetHomeDirectory()
		if err != nil {
			return nil // Can't check, allow it
		}
	}

	// Normalize the path
	normalizedPath, err := filepath.Abs(path)
	if err != nil {
		return nil
	}

	// Get relative path from home
	relPath, err := filepath.Rel(homeDir, normalizedPath)
	if err != nil {
		return nil // Not in home directory
	}

	e.logger.Debug().
		Str("path", path).
		Str("homeDir", homeDir).
		Str("relPath", relPath).
		Msg("Checking protected path")

	// Check exact match
	if e.config.Security.ProtectedPaths[relPath] {
		return errors.Newf(errors.ErrPermission,
			"cannot create symlink for protected file: %s", relPath)
	}

	// Check if within protected directory
	for protectedPath := range e.config.Security.ProtectedPaths {
		if strings.HasPrefix(relPath, protectedPath+"/") {
			return errors.Newf(errors.ErrPermission,
				"cannot create symlink for protected file: %s", relPath)
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
