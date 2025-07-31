package batch

import (
	"context"
	"fmt"
	"io/fs"
	"reflect"
	"strings"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
	"github.com/arthur-debert/synthfs/pkg/synthfs/execution"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
	"github.com/arthur-debert/synthfs/pkg/synthfs/targets"
	"github.com/arthur-debert/synthfs/pkg/synthfs/validation"
)

// BatchImpl represents a collection of operations that can be validated and executed as a unit.
// This implementation uses prerequisite resolution for automatic dependency management.
type BatchImpl struct {
	operations []interface{}
	fs         interface{} // Filesystem interface
	ctx        context.Context
	idCounter  int
	registry   core.OperationFactory
	logger     core.Logger
}

// NewBatch creates a new operation batch with prerequisite resolution enabled.
func NewBatch(fs interface{}, registry core.OperationFactory) Batch {
	return &BatchImpl{
		operations: []interface{}{},
		fs:         fs,
		ctx:        context.Background(),
		idCounter:  0,
		registry:   registry,
		logger:     nil, // Will be set by WithLogger method
	}
}

// Operations returns all operations currently in the batch.
func (b *BatchImpl) Operations() []interface{} {
	// Return a copy to prevent external modification
	opsCopy := make([]interface{}, len(b.operations))
	copy(opsCopy, b.operations)
	return opsCopy
}

// WithFileSystem sets the filesystem for the batch operations.
func (b *BatchImpl) WithFileSystem(fs interface{}) Batch {
	b.fs = fs
	return b
}

// WithContext sets the context for the batch operations.
func (b *BatchImpl) WithContext(ctx context.Context) Batch {
	b.ctx = ctx
	return b
}

// WithRegistry sets a custom operation registry for the batch.
func (b *BatchImpl) WithRegistry(registry core.OperationFactory) Batch {
	b.registry = registry
	return b
}

// WithLogger sets the logger for the batch.
func (b *BatchImpl) WithLogger(logger core.Logger) Batch {
	b.logger = logger
	return b
}

// add adds an operation to the batch with basic validation
func (b *BatchImpl) add(op interface{}) error {
	// Basic validation - prerequisites are handled by the execution pipeline
	if err := b.validateOperation(op); err != nil {
		return err
	}

	b.operations = append(b.operations, op)
	return nil
}

// validateOperation validates an operation
func (b *BatchImpl) validateOperation(op interface{}) error {
	// Try ValidateV2 first
	type validatorV2 interface {
		ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error
	}

	if v, ok := op.(validatorV2); ok {
		// Create a minimal ExecutionContext for validation
		execCtx := &core.ExecutionContext{}
		if err := v.ValidateV2(b.ctx, execCtx, b.fs); err != nil {
			return err
		}
		return nil
	}

	// Fallback to basic Validate method
	type validator interface {
		Validate(ctx context.Context, fsys interface{}) error
	}

	if v, ok := op.(validator); ok {
		if err := v.Validate(b.ctx, b.fs); err != nil {
			return err
		}
	}

	return nil
}

// CreateDir adds a directory creation operation to the batch.
func (b *BatchImpl) CreateDir(path string, mode ...fs.FileMode) (interface{}, error) {
	fileMode := fs.FileMode(0755) // Default directory mode
	if len(mode) > 0 {
		fileMode = mode[0]
	}

	// Create the operation using the registry
	op, err := b.createOperation("create_directory", path)
	if err != nil {
		return nil, err
	}

	// Create and set the directory item for this operation
	dirItem := targets.NewDirectory(path).WithMode(fileMode)
	if err := b.registry.SetItemForOperation(op, dirItem); err != nil {
		return nil, fmt.Errorf("failed to set item for CreateDir operation: %w", err)
	}

	// Set operation details
	if err := b.setOperationDetails(op, map[string]interface{}{
		"mode": fileMode.String(),
	}); err != nil {
		return nil, err
	}

	// Add to batch
	if err := b.add(op); err != nil {
		return nil, fmt.Errorf("validation failed for CreateDir(%s): %w", path, err)
	}

	return op, nil
}

// CreateFile adds a file creation operation to the batch.
func (b *BatchImpl) CreateFile(path string, content []byte, mode ...fs.FileMode) (interface{}, error) {
	fileMode := fs.FileMode(0644) // Default file mode
	if len(mode) > 0 {
		fileMode = mode[0]
	}

	// Create the operation
	op, err := b.createOperation("create_file", path)
	if err != nil {
		return nil, err
	}

	// Create and set the file item for this operation
	fileItem := targets.NewFile(path).WithContent(content).WithMode(fileMode)
	if err := b.registry.SetItemForOperation(op, fileItem); err != nil {
		return nil, fmt.Errorf("failed to set item for CreateFile operation: %w", err)
	}

	// Set operation details
	if err := b.setOperationDetails(op, map[string]interface{}{
		"content_length": len(content),
		"mode":           fileMode.String(),
		"content":        content,
	}); err != nil {
		return nil, err
	}

	// Add to batch
	if err := b.add(op); err != nil {
		return nil, fmt.Errorf("validation failed for CreateFile(%s): %w", path, err)
	}

	return op, nil
}

// Copy adds a copy operation to the batch.
func (b *BatchImpl) Copy(src, dst string) (interface{}, error) {
	// Create the operation
	op, err := b.createOperation("copy", src)
	if err != nil {
		return nil, err
	}

	// Set operation details
	if err := b.setOperationDetails(op, map[string]interface{}{
		"destination": dst,
		"src":         src,
		"dst":         dst,
	}); err != nil {
		return nil, err
	}

	// Set paths
	if err := b.setOperationPaths(op, src, dst); err != nil {
		return nil, err
	}

	// Add to batch
	if err := b.add(op); err != nil {
		return nil, fmt.Errorf("validation failed for Copy(%s, %s): %w", src, dst, err)
	}

	// Compute checksum for source file (after validation passes)
	if fs, ok := b.fs.(filesystem.FullFileSystem); ok {
		if checksum, err := validation.ComputeFileChecksum(fs, src); err == nil && checksum != nil {
			// Set checksum on operation
			type checksumSetter interface {
				SetChecksum(path string, checksum *validation.ChecksumRecord)
			}
			if setter, ok := op.(checksumSetter); ok {
				setter.SetChecksum(src, checksum)
			}
			// Set source_checksum in details
			_ = b.setOperationDetails(op, map[string]interface{}{
				"source_checksum": checksum.MD5,
			})
		}
	}

	return op, nil
}

// Move adds a move operation to the batch.
func (b *BatchImpl) Move(src, dst string) (interface{}, error) {
	// Create the operation
	op, err := b.createOperation("move", src)
	if err != nil {
		return nil, err
	}

	// Set operation details
	if err := b.setOperationDetails(op, map[string]interface{}{
		"destination": dst,
		"src":         src,
		"dst":         dst,
	}); err != nil {
		return nil, err
	}

	// Set paths
	if err := b.setOperationPaths(op, src, dst); err != nil {
		return nil, err
	}

	// Add to batch
	if err := b.add(op); err != nil {
		return nil, fmt.Errorf("validation failed for Move(%s, %s): %w", src, dst, err)
	}

	// Compute checksum for source file (after validation passes)
	if fs, ok := b.fs.(filesystem.FullFileSystem); ok {
		if checksum, err := validation.ComputeFileChecksum(fs, src); err == nil && checksum != nil {
			// Set checksum on operation
			type checksumSetter interface {
				SetChecksum(path string, checksum *validation.ChecksumRecord)
			}
			if setter, ok := op.(checksumSetter); ok {
				setter.SetChecksum(src, checksum)
			}
			// Set source_checksum in details
			_ = b.setOperationDetails(op, map[string]interface{}{
				"source_checksum": checksum.MD5,
			})
		}
	}

	return op, nil
}

// Delete adds a delete operation to the batch.
func (b *BatchImpl) Delete(path string) (interface{}, error) {
	// Create the operation
	op, err := b.createOperation("delete", path)
	if err != nil {
		return nil, err
	}

	// Add to batch
	if err := b.add(op); err != nil {
		return nil, fmt.Errorf("failed to add Delete(%s): %w", path, err)
	}

	return op, nil
}

// CreateSymlink adds a symbolic link creation operation to the batch.
func (b *BatchImpl) CreateSymlink(target, linkPath string) (interface{}, error) {
	// Create the operation
	op, err := b.createOperation("create_symlink", linkPath)
	if err != nil {
		return nil, err
	}

	// Create and set the symlink item for this operation
	symlinkItem := targets.NewSymlink(linkPath, target)
	if err := b.registry.SetItemForOperation(op, symlinkItem); err != nil {
		return nil, fmt.Errorf("failed to set item for CreateSymlink operation: %w", err)
	}

	// Set operation details
	if err := b.setOperationDetails(op, map[string]interface{}{
		"target": target,
	}); err != nil {
		return nil, err
	}

	// Add to batch
	if err := b.add(op); err != nil {
		return nil, fmt.Errorf("failed to add CreateSymlink(%s, %s): %w", target, linkPath, err)
	}

	return op, nil
}

// CreateArchive adds an archive creation operation to the batch.
func (b *BatchImpl) CreateArchive(archivePath string, format interface{}, sources ...string) (interface{}, error) {
	// Validate inputs
	if len(sources) == 0 {
		return nil, fmt.Errorf("validation failed for CreateArchive(%s): must specify at least one source", archivePath)
	}

	// Create the operation
	op, err := b.createOperation("create_archive", archivePath)
	if err != nil {
		return nil, err
	}

	// Set operation details
	if err := b.setOperationDetails(op, map[string]interface{}{
		"format":       format,
		"source_count": len(sources),
		"sources":      sources,
	}); err != nil {
		return nil, err
	}

	// Add to batch
	if err := b.add(op); err != nil {
		return nil, fmt.Errorf("failed to add CreateArchive(%s): %w", archivePath, err)
	}

	// Compute checksums for all source files
	if fs, ok := b.fs.(filesystem.FullFileSystem); ok {
		for _, source := range sources {
			if checksum, err := validation.ComputeFileChecksum(fs, source); err == nil && checksum != nil {
				// Set checksum on operation
				type checksumSetter interface {
					SetChecksum(path string, checksum *validation.ChecksumRecord)
				}
				if setter, ok := op.(checksumSetter); ok {
					setter.SetChecksum(source, checksum)
				}
			}
		}
		// Set sources_checksummed in details
		_ = b.setOperationDetails(op, map[string]interface{}{
			"sources_checksummed": len(sources),
		})
	}

	return op, nil
}

// Unarchive adds an unarchive operation to the batch.
func (b *BatchImpl) Unarchive(archivePath, extractPath string) (interface{}, error) {
	// Create the operation
	op, err := b.createOperation("unarchive", archivePath)
	if err != nil {
		return nil, err
	}

	// Create and set the unarchive item for this operation
	unarchiveItem := targets.NewUnarchive(archivePath, extractPath)
	if err := b.registry.SetItemForOperation(op, unarchiveItem); err != nil {
		return nil, fmt.Errorf("failed to set item for Unarchive operation: %w", err)
	}

	// Set operation details
	if err := b.setOperationDetails(op, map[string]interface{}{
		"extract_path": extractPath,
	}); err != nil {
		return nil, err
	}

	// Add to batch
	if err := b.add(op); err != nil {
		return nil, fmt.Errorf("failed to add Unarchive(%s, %s): %w", archivePath, extractPath, err)
	}

	return op, nil
}

// UnarchiveWithPatterns adds an unarchive operation with pattern filtering to the batch.
func (b *BatchImpl) UnarchiveWithPatterns(archivePath, extractPath string, patterns ...string) (interface{}, error) {
	// Create the operation
	op, err := b.createOperation("unarchive", archivePath)
	if err != nil {
		return nil, err
	}

	// Create and set the unarchive item for this operation with patterns
	unarchiveItem := targets.NewUnarchive(archivePath, extractPath).WithPatterns(patterns...)
	if err := b.registry.SetItemForOperation(op, unarchiveItem); err != nil {
		return nil, fmt.Errorf("failed to set item for UnarchiveWithPatterns operation: %w", err)
	}

	// Set operation details
	if err := b.setOperationDetails(op, map[string]interface{}{
		"extract_path":  extractPath,
		"patterns":      patterns,
		"pattern_count": len(patterns),
	}); err != nil {
		return nil, err
	}

	// Add to batch
	if err := b.add(op); err != nil {
		return nil, fmt.Errorf("failed to add UnarchiveWithPatterns(%s, %s): %w", archivePath, extractPath, err)
	}

	return op, nil
}

// Run runs all operations in the batch using default options with prerequisite resolution.
func (b *BatchImpl) Run() (interface{}, error) {
	return b.RunWithOptions(map[string]interface{}{
		"restorable":         false,
		"max_backup_size_mb": 0,
	})
}

// RunWithOptions runs all operations in the batch with specified options.
func (b *BatchImpl) RunWithOptions(opts interface{}) (interface{}, error) {
	startTime := time.Now()
	
	// Extract options and convert to core.PipelineOptions
	pipelineOpts := core.PipelineOptions{
		Restorable:           false,
		MaxBackupSizeMB:      10,
		ResolvePrerequisites: true, // Always enabled
	}
	
	if optsMap, ok := opts.(map[string]interface{}); ok {
		if r, ok := optsMap["restorable"].(bool); ok {
			pipelineOpts.Restorable = r
		}
		if mb, ok := optsMap["max_backup_size_mb"].(int); ok {
			pipelineOpts.MaxBackupSizeMB = mb
		}
	}
	
	// Log the start of execution
	if b.logger != nil {
		b.logger.Info().
			Int("operation_count", len(b.operations)).
			Bool("restorable", pipelineOpts.Restorable).
			Int("max_backup_mb", pipelineOpts.MaxBackupSizeMB).
			Bool("resolve_prerequisites", pipelineOpts.ResolvePrerequisites).
			Msg("executing batch")
	}
	
	// If no operations, return successful empty result
	if len(b.operations) == 0 {
		duration := time.Since(startTime)
		if b.logger != nil {
			b.logger.Info().
				Bool("success", true).
				Dur("duration", duration).
				Int("operations_executed", 0).
				Msg("batch execution completed")
		}
		
		batchResult := NewResult(true, b.operations, []interface{}{}, duration, nil)
		return batchResult, nil
	}
	
	// Create executor and pipeline using execution package
	loggerToUse := b.logger
	if loggerToUse == nil {
		// Create a no-op logger if none provided
		loggerToUse = &noOpLogger{}
	}
	executor := execution.NewExecutor(loggerToUse)
	
	// Create pipeline adapter
	pipeline := &pipelineAdapter{operations: b.operations, logger: loggerToUse}
	
	// Create prerequisite resolver (always enabled)
	prereqResolver := execution.NewPrerequisiteResolver(b.registry, loggerToUse)
	if b.logger != nil {
		b.logger.Info().Msg("created prerequisite resolver with operation factory")
	}
	
	// Execute using the execution package with prerequisite resolver
	coreResult := executor.RunWithOptionsAndResolver(b.ctx, pipeline, b.fs, pipelineOpts, prereqResolver)
	
	duration := time.Since(startTime)
	
	// Convert core.Result back to our interface{} result
	var executionError error
	if !coreResult.Success && len(coreResult.Errors) > 0 {
		executionError = coreResult.Errors[0] // Take first error
	}
	
	// Extract restore operations 
	var restoreOps []interface{}
	if coreResult.RestoreOps != nil {
		restoreOps = coreResult.RestoreOps
	}
	
	if b.logger != nil {
		b.logger.Info().
			Bool("success", coreResult.Success).
			Dur("duration", duration).
			Int("operations_executed", len(coreResult.Operations)).
			Int("restore_operations", len(restoreOps)).
			Msg("batch execution completed")
	}
	
	// Convert execution package results to interface{} slice
	var operationResults []interface{}
	for _, opResult := range coreResult.Operations {
		operationResults = append(operationResults, opResult)
	}
	
	// Convert to batch result interface
	batchResult := NewResultWithBudgetAndRollback(
		coreResult.Success,
		operationResults,
		restoreOps,
		duration,
		executionError,
		coreResult.Budget,
		coreResult.Rollback,
	)
	
	return batchResult, nil
}

// RunRestorable runs all operations with backup enabled using the default 10MB budget.
func (b *BatchImpl) RunRestorable() (interface{}, error) {
	return b.RunRestorableWithBudget(10)
}

// RunRestorableWithBudget runs all operations with backup enabled using a custom budget.
func (b *BatchImpl) RunRestorableWithBudget(maxBackupMB int) (interface{}, error) {
	opts := map[string]interface{}{
		"restorable":         true,
		"max_backup_size_mb": maxBackupMB,
	}
	return b.RunWithOptions(opts)
}

// Helper methods

// generateID creates a unique operation ID based on type and path.
func (b *BatchImpl) generateID(opType, path string) core.OperationID {
	b.idCounter++
	cleanPath := strings.ReplaceAll(path, "/", "_")
	cleanPath = strings.ReplaceAll(cleanPath, "\\", "_")
	return core.OperationID(fmt.Sprintf("batch_%d_%s_%s", b.idCounter, opType, cleanPath))
}

// createOperation is a helper method to create operations using the registry
func (b *BatchImpl) createOperation(opType, path string) (interface{}, error) {
	opID := b.generateID(opType, path)
	return b.registry.CreateOperation(opID, opType, path)
}

// setOperationDetails sets details on an operation through interface assertion
func (b *BatchImpl) setOperationDetails(op interface{}, details map[string]interface{}) error {
	type detailSetter interface {
		SetDescriptionDetail(key string, value interface{})
	}

	setter, ok := op.(detailSetter)
	if !ok {
		return fmt.Errorf("operation does not support setting details")
	}

	for key, value := range details {
		setter.SetDescriptionDetail(key, value)
	}

	return nil
}

// setOperationPaths sets paths on an operation through interface assertion
func (b *BatchImpl) setOperationPaths(op interface{}, src, dst string) error {
	type pathSetter interface {
		SetPaths(src, dst string)
	}

	setter, ok := op.(pathSetter)
	if !ok {
		return fmt.Errorf("operation does not support setting paths")
	}

	setter.SetPaths(src, dst)
	return nil
}

// pipelineAdapter adapts our operations to execution.PipelineInterface
type pipelineAdapter struct {
	operations []interface{}
	pipeline   execution.Pipeline
	logger     core.Logger
}

func (pa *pipelineAdapter) Add(ops ...interface{}) error {
	// This adapter doesn't support adding operations after creation
	return fmt.Errorf("pipelineAdapter does not support Add after creation")
}

func (pa *pipelineAdapter) Operations() []interface{} {
	// If we have a pipeline, return its operations (which includes resolved prerequisites)
	if pa.pipeline != nil {
		return pa.pipeline.Operations()
	}
	// Otherwise return the original operations
	return pa.operations
}

func (pa *pipelineAdapter) Resolve() error {
	if pa.pipeline == nil {
		// Use logger if available, otherwise use no-op logger
		logger := pa.logger
		if logger == nil {
			logger = &noOpLogger{}
		}
		pa.pipeline = execution.NewMemPipeline(logger)
		
		// Add all operations to the pipeline, wrapping them if needed
		for _, op := range pa.operations {
			
			// Check if operation already implements OperationInterface
			if _, ok := op.(execution.OperationInterface); ok {
				if err := pa.pipeline.Add(op); err != nil {
					return err
				}
			} else {
				// Wrap in operationAdapter
				wrapped := &operationAdapter{op: op}
				if err := pa.pipeline.Add(wrapped); err != nil {
					return err
				}
			}
		}
	}
	return pa.pipeline.Resolve()
}

func (pa *pipelineAdapter) ResolvePrerequisites(resolver core.PrerequisiteResolver, fs interface{}) error {
	if pa.pipeline == nil {
		// Use logger if available, otherwise use no-op logger
		logger := pa.logger
		if logger == nil {
			logger = &noOpLogger{}
		}
		pa.pipeline = execution.NewMemPipeline(logger)
		
		// Add all operations to the pipeline, wrapping them if needed
		for _, op := range pa.operations {
			
			// Check if operation already implements OperationInterface
			if _, ok := op.(execution.OperationInterface); ok {
				if err := pa.pipeline.Add(op); err != nil {
					return err
				}
			} else {
				// Wrap in operationAdapter
				wrapped := &operationAdapter{op: op}
				if err := pa.pipeline.Add(wrapped); err != nil {
					return err
				}
			}
		}
	}
	return pa.pipeline.ResolvePrerequisites(resolver, fs)
}

func (pa *pipelineAdapter) Validate(ctx context.Context, fs interface{}) error {
	if pa.pipeline == nil {
		// Use logger if available, otherwise use no-op logger
		logger := pa.logger
		if logger == nil {
			logger = &noOpLogger{}
		}
		pa.pipeline = execution.NewMemPipeline(logger)
		
		// Add all operations to the pipeline, wrapping them if needed
		for _, op := range pa.operations {
			
			// Check if operation already implements OperationInterface
			if _, ok := op.(execution.OperationInterface); ok {
				if err := pa.pipeline.Add(op); err != nil {
					return err
				}
			} else {
				// Wrap in operationAdapter
				wrapped := &operationAdapter{op: op}
				if err := pa.pipeline.Add(wrapped); err != nil {
					return err
				}
			}
		}
	}
	return pa.pipeline.Validate(ctx, fs)
}

// operationAdapter wraps interface{} operations to implement execution.OperationInterface
type operationAdapter struct {
	op interface{}
}

// GetOriginalOperation returns the wrapped operation
func (oa *operationAdapter) GetOriginalOperation() interface{} {
	return oa.op
}

func (oa *operationAdapter) ID() core.OperationID {
	if op, ok := oa.op.(interface{ ID() core.OperationID }); ok {
		return op.ID()
	}
	return core.OperationID("")
}

func (oa *operationAdapter) Describe() core.OperationDesc {
	if op, ok := oa.op.(interface{ Describe() core.OperationDesc }); ok {
		return op.Describe()
	}
	return core.OperationDesc{}
}

func (oa *operationAdapter) Dependencies() []core.OperationID {
	if op, ok := oa.op.(interface{ Dependencies() []core.OperationID }); ok {
		return op.Dependencies()
	}
	return []core.OperationID{}
}

func (oa *operationAdapter) Conflicts() []core.OperationID {
	if op, ok := oa.op.(interface{ Conflicts() []core.OperationID }); ok {
		return op.Conflicts()
	}
	return []core.OperationID{}
}

func (oa *operationAdapter) Prerequisites() []core.Prerequisite {
	if op, ok := oa.op.(interface{ Prerequisites() []core.Prerequisite }); ok {
		return op.Prerequisites()
	}
	return []core.Prerequisite{}
}

func (oa *operationAdapter) AddDependency(dep core.OperationID) {
	if op, ok := oa.op.(interface{ AddDependency(core.OperationID) }); ok {
		op.AddDependency(dep)
	}
}

func (oa *operationAdapter) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	// Try ExecuteV2 first
	if op, ok := oa.op.(interface{ ExecuteV2(interface{}, *core.ExecutionContext, interface{}) error }); ok {
		return op.ExecuteV2(ctx, execCtx, fsys)
	}
	
	// Fallback to Execute if available
	if op, ok := oa.op.(interface{ Execute(context.Context, interface{}) error }); ok {
		if ctxTyped, ok := ctx.(context.Context); ok {
			return op.Execute(ctxTyped, fsys)
		}
	}
	
	return fmt.Errorf("operation does not implement ExecuteV2 or Execute methods")
}

func (oa *operationAdapter) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	// Try ValidateV2 first
	if op, ok := oa.op.(interface{ ValidateV2(interface{}, *core.ExecutionContext, interface{}) error }); ok {
		return op.ValidateV2(ctx, execCtx, fsys)
	}
	
	// Fallback to Validate if available
	if op, ok := oa.op.(interface{ Validate(context.Context, interface{}) error }); ok {
		if ctxTyped, ok := ctx.(context.Context); ok {
			return op.Validate(ctxTyped, fsys)
		}
	}
	
	return nil // No validation available
}

func (oa *operationAdapter) ReverseOps(ctx context.Context, fsys interface{}, budget *core.BackupBudget) ([]interface{}, *core.BackupData, error) {
	// Use reflection to call ReverseOps dynamically
	opValue := reflect.ValueOf(oa.op)
	method := opValue.MethodByName("ReverseOps")
	
	if method.IsValid() {
		// Call the method with appropriate arguments
		args := []reflect.Value{
			reflect.ValueOf(ctx),
			reflect.ValueOf(fsys),
			reflect.ValueOf(budget),
		}
		
		results := method.Call(args)
		if len(results) == 3 {
			// Extract the results
			var reverseOps []interface{}
			if !results[0].IsNil() {
				// Convert the slice to []interface{}
				slice := results[0]
				for i := 0; i < slice.Len(); i++ {
					reverseOps = append(reverseOps, slice.Index(i).Interface())
				}
			}
			
			var backupData *core.BackupData
			if !results[1].IsNil() {
				if bd, ok := results[1].Interface().(*core.BackupData); ok {
					backupData = bd
				}
			}
			
			var err error
			if !results[2].IsNil() {
				if e, ok := results[2].Interface().(error); ok {
					err = e
				}
			}
			
			return reverseOps, backupData, err
		}
	}
	
	// Fallback: Try the interface{} budget signature (used by operations package)
	if op, ok := oa.op.(interface{ ReverseOps(context.Context, interface{}, interface{}) ([]interface{}, interface{}, error) }); ok {
		reverseOps, backupData, err := op.ReverseOps(ctx, fsys, budget)
		// Convert backupData from interface{} to *core.BackupData
		if backupData != nil {
			if bd, ok := backupData.(*core.BackupData); ok {
				return reverseOps, bd, err
			}
		}
		return reverseOps, nil, err
	}
	
	return nil, nil, nil
}

func (oa *operationAdapter) Rollback(ctx context.Context, fsys interface{}) error {
	if op, ok := oa.op.(interface{ Rollback(context.Context, interface{}) error }); ok {
		return op.Rollback(ctx, fsys)
	}
	return nil
}

func (oa *operationAdapter) GetItem() interface{} {
	if op, ok := oa.op.(interface{ GetItem() interface{} }); ok {
		return op.GetItem()
	}
	return nil
}

func (oa *operationAdapter) SetDescriptionDetail(key string, value interface{}) {
	if op, ok := oa.op.(interface{ SetDescriptionDetail(string, interface{}) }); ok {
		op.SetDescriptionDetail(key, value)
	}
}

// noOpLogger implements core.Logger for when no logger is provided
type noOpLogger struct{}

func (l *noOpLogger) Trace() core.LogEvent { return &noOpLogEvent{} }
func (l *noOpLogger) Debug() core.LogEvent { return &noOpLogEvent{} }
func (l *noOpLogger) Info() core.LogEvent  { return &noOpLogEvent{} }
func (l *noOpLogger) Warn() core.LogEvent  { return &noOpLogEvent{} }
func (l *noOpLogger) Error() core.LogEvent { return &noOpLogEvent{} }

type noOpLogEvent struct{}

func (e *noOpLogEvent) Str(key, val string) core.LogEvent             { return e }
func (e *noOpLogEvent) Int(key string, val int) core.LogEvent         { return e }
func (e *noOpLogEvent) Bool(key string, val bool) core.LogEvent       { return e }
func (e *noOpLogEvent) Dur(key string, val interface{}) core.LogEvent { return e }
func (e *noOpLogEvent) Interface(key string, val interface{}) core.LogEvent { return e }
func (e *noOpLogEvent) Err(err error) core.LogEvent                   { return e }
func (e *noOpLogEvent) Float64(key string, val float64) core.LogEvent { return e }
func (e *noOpLogEvent) Msg(msg string)                                {}

