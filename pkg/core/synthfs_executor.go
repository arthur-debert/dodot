package core

import (
	"context"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
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
	logger     zerolog.Logger
	dryRun     bool
	filesystem synthfs.FileSystem
	paths      *paths.Paths
}

// NewSynthfsExecutor creates a new synthfs-based executor
func NewSynthfsExecutor(dryRun bool) *SynthfsExecutor {
	// Initialize paths with empty string to use defaults
	p, _ := paths.New("")
	return &SynthfsExecutor{
		logger:     logging.GetLogger("core.synthfs"),
		dryRun:     dryRun,
		filesystem: filesystem.NewOSFileSystem("/"), // Use root filesystem
		paths:      p,
	}
}

// NewSynthfsExecutorWithPaths creates a new synthfs-based executor with custom paths
func NewSynthfsExecutorWithPaths(dryRun bool, p *paths.Paths) *SynthfsExecutor {
	return &SynthfsExecutor{
		logger:     logging.GetLogger("core.synthfs"),
		dryRun:     dryRun,
		filesystem: filesystem.NewOSFileSystem("/"), // Use root filesystem
		paths:      p,
	}
}

// ExecuteOperations executes a list of operations using synthfs
func (e *SynthfsExecutor) ExecuteOperations(ops []types.Operation) error {
	if e.dryRun {
		e.logger.Info().Msg("Dry run mode - operations would be executed:")
		for _, op := range ops {
			e.logOperation(op)
		}
		return nil
	}

	// Convert dodot operations to synthfs operations
	synthOps := make([]synthfs.Operation, 0, len(ops))
	for _, op := range ops {
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

	// For issue #70, we only allow symlinks in safe directories
	// Issue #71 will handle symlinks in user home directory
	if err := e.validateSafePath(op.Target); err != nil {
		return nil, err
	}

	e.logger.Debug().
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
	return rel != ".." && !filepath.IsAbs(rel) && rel[0] != '.'
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
