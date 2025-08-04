package synthfs

import (
	"context"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// Run executes a series of operations in sequence.
//
// Operations are executed in the order provided, with each operation's success required
// before proceeding. If an operation fails, subsequent operations are not executed.
// By default, this function does not perform a rollback. To enable rollback,
// use RunWithOptions with RollbackOnError set to true.
//
// Example - Simple sequential operations:
//
//	fs := synthfs.NewOSFileSystem("/tmp")
//	sfs := synthfs.New()
//
//	result, err := synthfs.Run(ctx, fs,
//		sfs.CreateDir("project", 0755),
//		sfs.CreateFile("project/README.md", []byte("# Project"), 0644),
//		sfs.CreateFile("project/main.go", []byte("package main"), 0644),
//	)
//
//	if err != nil {
//		log.Fatal(err)
//	}
//	log.Printf("Executed %d operations in %v", len(result.Operations), result.Duration)
//
// Example - Operations with dependencies:
//
//	result, err := synthfs.Run(ctx, fs,
//		sfs.CreateDir("data", 0755),                    // Must happen first
//		sfs.CreateFile("data/config.json", data, 0644), // Depends on directory existing
//		sfs.Copy("data/config.json", "backup.json"),    // Depends on file existing
//	)
//
// For complex dependency management, consider using BuildPipeline instead.
func Run(ctx context.Context, fs filesystem.FullFileSystem, ops ...Operation) (*Result, error) {
	return RunWithOptions(ctx, fs, DefaultPipelineOptions(), ops...)
}

// RunWithOptions executes operations with custom options.
// Note: This simplified runner does not currently support all pipeline options (e.g., DryRun).
// It executes operations sequentially and does not perform a pre-validation step for the entire pipeline.
func RunWithOptions(ctx context.Context, fs filesystem.FullFileSystem, options PipelineOptions, ops ...Operation) (*Result, error) {
	if options.DryRun {
		fs = NewDryRunFS()
	}

	if len(ops) == 0 {
		return &Result{
			success:    true,
			operations: []interface{}{},
			duration:   0,
		}, nil
	}

	results := make([]interface{}, 0, len(ops))
	successfulOps := make([]Operation, 0, len(ops))
	startTime := time.Now()

	for i, op := range ops {
		// Validate before execute
		if err := op.Validate(ctx, fs); err != nil {
			return handleOpError(ctx, fs, options, err, op, i, ops, results, successfulOps, startTime, true)
		}

		// Execute operation
		startOpTime := time.Now()
		err := op.Execute(ctx, fs)
		duration := time.Since(startOpTime)

		opResult := OperationResult{
			OperationID: op.ID(),
			Operation:   op,
			Status:      StatusSuccess,
			Error:       err,
			Duration:    duration,
		}

		if err != nil {
			opResult.Status = StatusFailure
			results = append(results, opResult)
			return handleOpError(ctx, fs, options, err, op, i, ops, results, successfulOps, startTime, false)
		}

		results = append(results, opResult)
		successfulOps = append(successfulOps, op)
	}

	return &Result{
		success:    true,
		operations: results,
		duration:   time.Since(startTime),
	}, nil
}

func handleOpError(
	ctx context.Context,
	fs FileSystem,
	options PipelineOptions,
	err error,
	op Operation,
	i int,
	ops []Operation,
	results []interface{},
	successfulOps []Operation,
	startTime time.Time,
	isValidation bool,
) (*Result, error) {
	if isValidation {
		opResult := OperationResult{
			OperationID: op.ID(),
			Operation:   op,
			Status:      StatusValidation,
			Error:       err,
			Duration:    0,
		}
		results = append(results, opResult)
	}

	// Attempt rollback if requested
	if options.RollbackOnError {
		rollbackErrs := make(map[OperationID]error)
		for k := len(successfulOps) - 1; k >= 0; k-- {
			opToRollback := successfulOps[k]
			if rollbackErr := opToRollback.Rollback(ctx, fs); rollbackErr != nil {
				rollbackErrs[opToRollback.ID()] = rollbackErr
			}
		}
		if len(rollbackErrs) > 0 {
			err = &RollbackError{
				OriginalErr:  err,
				RollbackErrs: rollbackErrs,
			}
		}
	}

	var successfulOpIDs []OperationID
	for _, successfulOp := range successfulOps {
		successfulOpIDs = append(successfulOpIDs, successfulOp.ID())
	}

	pipelineErr := &PipelineError{
		FailedOp:      op,
		FailedIndex:   i + 1,
		TotalOps:      len(ops),
		Err:           err,
		SuccessfulOps: successfulOpIDs,
	}

	return &Result{
		success:    false,
		operations: results,
		duration:   time.Since(startTime),
		err:        err,
	}, pipelineErr
}
