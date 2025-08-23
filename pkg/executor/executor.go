package executor

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/utils"
	"github.com/rs/zerolog"
)

// Options contains configuration for the executor
type Options struct {
	DataStore datastore.DataStore
	DryRun    bool
	Logger    zerolog.Logger
	// Filesystem operations interface for testing
	FS types.FS
}

// Executor is the simplified executor that processes Action instances
type Executor struct {
	dataStore datastore.DataStore
	dryRun    bool
	logger    zerolog.Logger
	fs        types.FS
}

// New creates a new executor instance
func New(opts Options) *Executor {
	logger := opts.Logger
	if logger.GetLevel() == zerolog.Disabled {
		logger = logging.GetLogger("executor")
	}

	fs := opts.FS
	if fs == nil {
		fs = filesystem.NewOS()
	}

	return &Executor{
		dataStore: opts.DataStore,
		dryRun:    opts.DryRun,
		logger:    logger,
		fs:        fs,
	}
}

// Execute processes a slice of actions and returns their results
func (e *Executor) Execute(actions []types.Action) []types.ActionResult {
	results := make([]types.ActionResult, 0, len(actions))

	for _, action := range actions {
		result := e.executeAction(action)
		results = append(results, result)
	}

	return results
}

// executeAction executes a single action and returns its result
func (e *Executor) executeAction(action types.Action) types.ActionResult {
	start := time.Now()

	e.logger.Debug().
		Str("action", fmt.Sprintf("%T", action)).
		Str("pack", action.Pack()).
		Str("description", action.Description()).
		Bool("dry_run", e.dryRun).
		Msg("Executing action")

	if e.dryRun {
		return types.ActionResult{
			Action:   action,
			Success:  true,
			Skipped:  true,
			Message:  "Dry run - no changes made",
			Duration: time.Since(start),
		}
	}

	// Execute the action through the datastore
	err := action.Execute(e.dataStore)
	if err != nil {
		e.logger.Error().
			Err(err).
			Str("action", fmt.Sprintf("%T", action)).
			Msg("Action execution failed")

		return types.ActionResult{
			Action:   action,
			Success:  false,
			Error:    err,
			Duration: time.Since(start),
		}
	}

	// Handle special post-execution logic for specific action types
	err = e.handlePostExecution(action)
	if err != nil {
		e.logger.Error().
			Err(err).
			Str("action", fmt.Sprintf("%T", action)).
			Msg("Post-execution handling failed")

		return types.ActionResult{
			Action:   action,
			Success:  false,
			Error:    err,
			Duration: time.Since(start),
		}
	}

	e.logger.Info().
		Str("action", fmt.Sprintf("%T", action)).
		Str("pack", action.Pack()).
		Dur("duration", time.Since(start)).
		Msg("Action executed successfully")

	return types.ActionResult{
		Action:   action,
		Success:  true,
		Duration: time.Since(start),
	}
}

// handlePostExecution handles any special logic needed after action execution
func (e *Executor) handlePostExecution(action types.Action) error {
	switch a := action.(type) {
	case *types.LinkAction:
		return e.handleLinkActionPost(a)
	case *types.RunScriptAction:
		return e.handleRunScriptActionPost(a)
	case *types.BrewAction:
		return e.handleBrewActionPost(a)
	default:
		// No special handling needed for other action types
		return nil
	}
}

// handleLinkActionPost creates the final user-facing symlink
func (e *Executor) handleLinkActionPost(action *types.LinkAction) error {
	// Get the intermediate path by calling the datastore again
	// This is idempotent - it will return the existing link
	intermediatePath, err := e.dataStore.Link(action.PackName, action.SourceFile)
	if err != nil {
		return fmt.Errorf("failed to get intermediate link path: %w", err)
	}

	// Expand the target path
	targetPath := utils.ExpandPath(action.TargetFile)

	// Create parent directory if needed
	targetDir := filepath.Dir(targetPath)
	if err := e.fs.MkdirAll(targetDir, 0755); err != nil {
		return fmt.Errorf("failed to create target directory %s: %w", targetDir, err)
	}

	// Remove existing link/file if it exists
	if _, err := e.fs.Lstat(targetPath); err == nil {
		if err := e.fs.Remove(targetPath); err != nil {
			return fmt.Errorf("failed to remove existing file at %s: %w", targetPath, err)
		}
	}

	// Create the final symlink
	if err := e.fs.Symlink(intermediatePath, targetPath); err != nil {
		return fmt.Errorf("failed to create symlink from %s to %s: %w", targetPath, intermediatePath, err)
	}

	e.logger.Debug().
		Str("source", action.SourceFile).
		Str("intermediate", intermediatePath).
		Str("target", targetPath).
		Msg("Created final symlink")

	return nil
}

// handleRunScriptActionPost executes the provisioning script
func (e *Executor) handleRunScriptActionPost(action *types.RunScriptAction) error {
	// The action has already checked if provisioning is needed
	// If we're here and no error occurred, we need to run the script

	// Check again if we need to run (action might have returned early)
	needs, err := e.dataStore.NeedsProvisioning(action.PackName, action.SentinelName, action.Checksum)
	if err != nil {
		return fmt.Errorf("failed to check provisioning status: %w", err)
	}

	if !needs {
		e.logger.Debug().
			Str("script", action.ScriptPath).
			Str("pack", action.PackName).
			Msg("Script already provisioned, skipping execution")
		return nil
	}

	// Execute the script
	e.logger.Info().
		Str("script", action.ScriptPath).
		Str("pack", action.PackName).
		Msg("Executing provisioning script")

	cmd := exec.Command("/bin/sh", action.ScriptPath)
	cmd.Dir = filepath.Dir(action.ScriptPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	if err := cmd.Run(); err != nil {
		return fmt.Errorf("failed to execute script %s: %w", action.ScriptPath, err)
	}

	// Record successful provisioning
	if err := e.dataStore.RecordProvisioning(action.PackName, action.SentinelName, action.Checksum); err != nil {
		return fmt.Errorf("failed to record provisioning: %w", err)
	}

	e.logger.Info().
		Str("script", action.ScriptPath).
		Str("pack", action.PackName).
		Msg("Provisioning script executed successfully")

	return nil
}

// handleBrewActionPost executes brew bundle for a Brewfile
func (e *Executor) handleBrewActionPost(action *types.BrewAction) error {
	// The action has already checked if provisioning is needed
	// If we're here and no error occurred, we need to run brew bundle

	// Check again if we need to run (action might have returned early)
	sentinelName := fmt.Sprintf("homebrew-%s.sentinel", action.PackName)
	needs, err := e.dataStore.NeedsProvisioning(action.PackName, sentinelName, action.Checksum)
	if err != nil {
		return fmt.Errorf("failed to check provisioning status: %w", err)
	}

	if !needs {
		e.logger.Debug().
			Str("brewfile", action.BrewfilePath).
			Str("pack", action.PackName).
			Msg("Brewfile already provisioned, skipping execution")
		return nil
	}

	// Execute brew bundle
	e.logger.Info().
		Str("brewfile", action.BrewfilePath).
		Str("pack", action.PackName).
		Msg("Executing brew bundle")

	cmd := exec.Command("brew", "bundle", "--file="+action.BrewfilePath)
	cmd.Dir = filepath.Dir(action.BrewfilePath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	if err := cmd.Run(); err != nil {
		return fmt.Errorf("failed to execute brew bundle for %s: %w", action.BrewfilePath, err)
	}

	// Record successful provisioning
	if err := e.dataStore.RecordProvisioning(action.PackName, sentinelName, action.Checksum); err != nil {
		return fmt.Errorf("failed to record provisioning: %w", err)
	}

	e.logger.Info().
		Str("brewfile", action.BrewfilePath).
		Str("pack", action.PackName).
		Msg("Brew bundle executed successfully")

	return nil
}
