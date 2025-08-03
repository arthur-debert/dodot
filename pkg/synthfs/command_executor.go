package synthfs

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// CommandExecutor handles execution of command operations
type CommandExecutor struct {
	logger zerolog.Logger
	dryRun bool
}

// NewCommandExecutor creates a new command executor
func NewCommandExecutor(dryRun bool) *CommandExecutor {
	return &CommandExecutor{
		logger: logging.GetLogger("core.command_executor"),
		dryRun: dryRun,
	}
}

// ExecuteOperations executes only OperationExecute type operations
func (e *CommandExecutor) ExecuteOperations(ops []types.Operation) error {
	for _, op := range ops {
		if op.Type != types.OperationExecute {
			continue
		}

		if op.Status != types.StatusReady {
			e.logger.Debug().
				Str("command", op.Command).
				Str("status", string(op.Status)).
				Msg("Skipping operation with non-ready status")
			continue
		}

		if err := e.executeOperation(op); err != nil {
			return err
		}
	}
	return nil
}

// executeOperation executes a single command operation
func (e *CommandExecutor) executeOperation(op types.Operation) error {
	if op.Command == "" {
		return errors.New(errors.ErrInvalidInput, "execute operation requires command")
	}

	e.logger.Info().
		Str("command", op.Command).
		Strs("args", op.Args).
		Str("workingDir", op.WorkingDir).
		Str("description", op.Description).
		Msg("Executing command")

	if e.dryRun {
		e.logger.Info().Msg("Dry run mode - command would be executed")
		return nil
	}

	// Create command
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()

	cmd := exec.CommandContext(ctx, op.Command, op.Args...)

	// Set working directory if specified
	if op.WorkingDir != "" {
		// Ensure working directory exists
		if _, err := os.Stat(op.WorkingDir); os.IsNotExist(err) {
			return errors.Newf(errors.ErrFileAccess,
				"working directory does not exist: %s", op.WorkingDir)
		}
		cmd.Dir = op.WorkingDir
	}

	// Set environment variables
	cmd.Env = os.Environ() // Start with current environment
	for key, value := range op.EnvironmentVars {
		cmd.Env = append(cmd.Env, fmt.Sprintf("%s=%s", key, value))
	}

	// Add some useful environment variables
	if op.WorkingDir != "" {
		// Get pack name from working directory (last component)
		packName := filepath.Base(op.WorkingDir)
		cmd.Env = append(cmd.Env, fmt.Sprintf("DODOT_PACK=%s", packName))
		cmd.Env = append(cmd.Env, fmt.Sprintf("DODOT_PACK_DIR=%s", op.WorkingDir))
	}

	// Capture output
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	// Execute command
	err := cmd.Run()

	// Display output to user
	if stdout.Len() > 0 {
		// Print stdout directly to the console so users can see script progress
		fmt.Print(stdout.String())

		// Also log for debugging
		e.logger.Debug().
			Str("output", stdout.String()).
			Msg("Command stdout")
	}
	if stderr.Len() > 0 {
		// Print stderr to console as well
		fmt.Fprint(os.Stderr, stderr.String())

		// Also log for debugging
		e.logger.Debug().
			Str("output", stderr.String()).
			Msg("Command stderr")
	}

	if err != nil {
		e.logger.Error().
			Err(err).
			Str("command", op.Command).
			Strs("args", op.Args).
			Str("stdout", stdout.String()).
			Str("stderr", stderr.String()).
			Msg("Command execution failed")

		return errors.Wrapf(err, errors.ErrActionExecute,
			"failed to execute command: %s", op.Command)
	}

	e.logger.Info().
		Str("command", op.Command).
		Msg("Command executed successfully")

	return nil
}

// Result represents the result of a command execution
type Result struct {
	Success bool
	Stdout  string
	Stderr  string
	Error   error
}

// ExecuteWithResult executes a single operation and returns detailed result
func (e *CommandExecutor) ExecuteWithResult(op types.Operation) Result {
	if op.Type != types.OperationExecute {
		return Result{
			Success: false,
			Error:   errors.Newf(errors.ErrInvalidInput, "operation type must be execute, got: %s", op.Type),
		}
	}

	if e.dryRun {
		e.logger.Info().
			Str("command", op.Command).
			Msg("Dry run mode - command would be executed")
		return Result{Success: true}
	}

	// Create command
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()

	cmd := exec.CommandContext(ctx, op.Command, op.Args...)

	// Set working directory if specified
	if op.WorkingDir != "" {
		cmd.Dir = op.WorkingDir
	}

	// Set environment variables
	cmd.Env = os.Environ()
	for key, value := range op.EnvironmentVars {
		cmd.Env = append(cmd.Env, fmt.Sprintf("%s=%s", key, value))
	}

	// Capture output
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	// Execute command
	err := cmd.Run()

	result := Result{
		Success: err == nil,
		Stdout:  stdout.String(),
		Stderr:  stderr.String(),
		Error:   err,
	}

	if err != nil {
		result.Error = errors.Wrapf(err, errors.ErrActionExecute,
			"command failed: %s", op.Command)
	}

	return result
}
