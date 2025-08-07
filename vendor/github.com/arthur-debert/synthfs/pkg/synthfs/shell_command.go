package synthfs

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// ShellCommandOptions configures how a shell command is executed
type ShellCommandOptions struct {
	// WorkDir sets the working directory for the command
	WorkDir string

	// Env sets environment variables for the command (in addition to current environment)
	Env map[string]string

	// Timeout sets a timeout for command execution
	Timeout time.Duration

	// CaptureOutput determines if stdout/stderr should be captured
	CaptureOutput bool

	// RollbackCommand is an optional command to run on rollback
	RollbackCommand string

	// Shell specifies the shell to use (defaults to sh on Unix, cmd on Windows)
	Shell string

	// ShellArgs are additional arguments to pass to the shell (defaults to -c)
	ShellArgs []string
}

// ShellCommandOption is a function that configures ShellCommandOptions
type ShellCommandOption func(*ShellCommandOptions)

// WithWorkDir sets the working directory for the command
func WithWorkDir(dir string) ShellCommandOption {
	return func(opts *ShellCommandOptions) {
		opts.WorkDir = dir
	}
}

// WithEnv sets environment variables for the command
func WithEnv(env map[string]string) ShellCommandOption {
	return func(opts *ShellCommandOptions) {
		opts.Env = env
	}
}

// WithTimeout sets a timeout for command execution
func WithTimeout(timeout time.Duration) ShellCommandOption {
	return func(opts *ShellCommandOptions) {
		opts.Timeout = timeout
	}
}

// WithCaptureOutput enables capturing of stdout/stderr
func WithCaptureOutput() ShellCommandOption {
	return func(opts *ShellCommandOptions) {
		opts.CaptureOutput = true
	}
}

// WithRollbackCommand sets a command to run on rollback
func WithRollbackCommand(cmd string) ShellCommandOption {
	return func(opts *ShellCommandOptions) {
		opts.RollbackCommand = cmd
	}
}

// WithShell sets the shell to use for command execution
func WithShell(shell string, args ...string) ShellCommandOption {
	return func(opts *ShellCommandOptions) {
		opts.Shell = shell
		opts.ShellArgs = args
	}
}

// defaultShellCommandOptions returns default options for shell commands
func defaultShellCommandOptions() *ShellCommandOptions {
	opts := &ShellCommandOptions{
		CaptureOutput: false, // Default to not capturing unless explicitly requested
		Timeout:       2 * time.Minute,
		Env:           make(map[string]string),
	}

	// Set default shell based on OS
	if strings.Contains(strings.ToLower(os.Getenv("OS")), "windows") {
		opts.Shell = "cmd"
		opts.ShellArgs = []string{"/c"}
	} else {
		opts.Shell = "sh"
		opts.ShellArgs = []string{"-c"}
	}

	return opts
}

// executeShellCommand is the core execution logic
func executeShellCommand(ctx context.Context, command string, opts *ShellCommandOptions) (stdout, stderr string, err error) {
	// Build command arguments
	args := append(opts.ShellArgs, command)
	cmd := exec.CommandContext(ctx, opts.Shell, args...)

	// Set working directory if specified
	if opts.WorkDir != "" {
		cmd.Dir = opts.WorkDir
	}

	// Set environment variables
	if len(opts.Env) > 0 {
		cmd.Env = os.Environ()
		for k, v := range opts.Env {
			cmd.Env = append(cmd.Env, fmt.Sprintf("%s=%s", k, v))
		}
	}

	// Capture output if requested
	var stdoutBuf, stderrBuf bytes.Buffer
	if opts.CaptureOutput {
		cmd.Stdout = &stdoutBuf
		cmd.Stderr = &stderrBuf
	} else {
		cmd.Stdout = os.Stdout
		cmd.Stderr = os.Stderr
	}

	// Apply timeout if specified
	if opts.Timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, opts.Timeout)
		defer cancel()
		cmd = exec.CommandContext(ctx, opts.Shell, args...)
		// Re-apply all settings to new command
		if opts.WorkDir != "" {
			cmd.Dir = opts.WorkDir
		}
		if len(opts.Env) > 0 {
			cmd.Env = os.Environ()
			for k, v := range opts.Env {
				cmd.Env = append(cmd.Env, fmt.Sprintf("%s=%s", k, v))
			}
		}
		if opts.CaptureOutput {
			cmd.Stdout = &stdoutBuf
			cmd.Stderr = &stderrBuf
		} else {
			cmd.Stdout = os.Stdout
			cmd.Stderr = os.Stderr
		}
	}

	// Execute command
	err = cmd.Run()
	if err != nil {
		errMsg := fmt.Sprintf("command failed: %v", err)
		if opts.CaptureOutput && stderrBuf.Len() > 0 {
			errMsg += fmt.Sprintf("\nstderr: %s", stderrBuf.String())
		}
		return stdoutBuf.String(), stderrBuf.String(), fmt.Errorf("%s", errMsg)
	}

	return stdoutBuf.String(), stderrBuf.String(), nil
}

// createShellCommand creates a shell command operation
func createShellCommand(id, command string, options ...ShellCommandOption) *CustomOperation {
	opts := defaultShellCommandOptions()
	for _, opt := range options {
		opt(opts)
	}

	var op *CustomOperation

	if opts.CaptureOutput {
		// Use version with output capture
		op = NewCustomOperationWithOutput(id, func(ctx context.Context, fs filesystem.FileSystem, storeOutput func(string, interface{})) error {
			stdout, stderr, err := executeShellCommand(ctx, command, opts)

			// Store captured output
			if stdout != "" {
				storeOutput("stdout", stdout)
			}
			if stderr != "" {
				storeOutput("stderr", stderr)
			}

			return err
		})
	} else {
		// Use regular version without output capture
		op = NewCustomOperation(id, func(ctx context.Context, fs filesystem.FileSystem) error {
			_, _, err := executeShellCommand(ctx, command, opts)
			return err
		})
	}

	op = op.WithDescription(fmt.Sprintf("Execute shell command: %s", command))

	// Add rollback if specified
	if opts.RollbackCommand != "" {
		rollbackFunc := func(ctx context.Context, fs filesystem.FileSystem) error {
			// Use same options for rollback command
			rollbackOpts := *opts
			rollbackOpts.RollbackCommand = "" // Prevent infinite recursion

			rollbackOp := createShellCommand(
				fmt.Sprintf("rollback_%s", id),
				opts.RollbackCommand,
				func(o *ShellCommandOptions) { *o = rollbackOpts },
			)

			return rollbackOp.Execute(ctx, fs)
		}
		op = op.WithRollback(rollbackFunc)
	}

	return op
}

// ShellCommand creates a shell command operation with auto-generated ID
func (s *SynthFS) ShellCommand(command string, options ...ShellCommandOption) Operation {
	id := s.idGen("shell_command", command)
	op := createShellCommand(string(id), command, options...)
	return NewCustomOperationAdapter(op)
}

// ShellCommandWithID creates a shell command operation with explicit ID
func (s *SynthFS) ShellCommandWithID(id, command string, options ...ShellCommandOption) Operation {
	op := createShellCommand(id, command, options...)
	return NewCustomOperationAdapter(op)
}
