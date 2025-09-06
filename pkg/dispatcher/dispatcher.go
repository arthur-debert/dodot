// Package dispatcher provides centralized command dispatching for pack operations.
// It acts as the entry point from the CLI layer, eliminating the need for
// individual command packages.
package dispatcher

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/logging"
	packcommands "github.com/arthur-debert/dodot/pkg/packs/commands"
	"github.com/arthur-debert/dodot/pkg/packs/orchestration"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/display"
)

// CommandType represents the type of pack command being executed
type CommandType string

const (
	// Core commands
	CommandUp     CommandType = "up"
	CommandDown   CommandType = "down"
	CommandStatus CommandType = "status"

	// Single pack convenience commands
	CommandInit      CommandType = "init"
	CommandFill      CommandType = "fill"
	CommandAdopt     CommandType = "adopt"
	CommandAddIgnore CommandType = "add-ignore"
)

// Options contains all possible options for pack commands.
// Each command will use only the fields it needs.
type Options struct {
	// Common fields
	DotfilesRoot string
	PackNames    []string
	DryRun       bool
	Force        bool
	FileSystem   types.FS

	// Command-specific fields
	// For on command
	NoProvision    bool
	ProvisionRerun bool

	// For adopt command
	SourcePaths []string

	// For init command
	PackName string // Single pack name for init

	// For status command
	Paths paths.Paths
}

// Dispatch is the central dispatcher for all pack-related commands.
// It replaces the individual command packages by directly calling the appropriate
// pack functions based on the command type.
func Dispatch(cmdType CommandType, opts Options) (*display.PackCommandResult, error) {
	logger := logging.GetLogger("dispatcher")
	logger.Debug().
		Str("command", string(cmdType)).
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Msg("Dispatching pack command")

	switch cmdType {
	case CommandUp:
		// Use pack pipeline for on command
		onCmd := &packcommands.UpCommand{
			NoProvision: opts.NoProvision,
			Force:       opts.Force,
		}
		pipelineResult, err := orchestration.Execute(onCmd, opts.PackNames, orchestration.Options{
			DotfilesRoot: opts.DotfilesRoot,
			DryRun:       opts.DryRun,
			FileSystem:   opts.FileSystem,
		})
		if err != nil {
			return nil, err
		}
		return convertPipelineResult(pipelineResult), nil

	case CommandDown:
		// Use pack pipeline for off command
		offCmd := &packcommands.DownCommand{}
		pipelineResult, err := orchestration.Execute(offCmd, opts.PackNames, orchestration.Options{
			DotfilesRoot: opts.DotfilesRoot,
			DryRun:       opts.DryRun,
			FileSystem:   opts.FileSystem,
		})
		if err != nil {
			return nil, err
		}
		return convertPipelineResult(pipelineResult), nil

	case CommandStatus:
		// Use pack pipeline for status command
		statusCmd := &packcommands.StatusCommand{}
		pipelineResult, err := orchestration.Execute(statusCmd, opts.PackNames, orchestration.Options{
			DotfilesRoot: opts.DotfilesRoot,
			DryRun:       false, // Status is always a query
			FileSystem:   opts.FileSystem,
		})
		if err != nil {
			return nil, err
		}
		return convertPipelineResult(pipelineResult), nil

	case CommandInit:
		// Init is special - it creates the pack first, then fills it
		// Step 1: Create the pack directory
		err := packcommands.InitPreprocess(opts.PackName, opts.DotfilesRoot, opts.FileSystem)
		if err != nil {
			return nil, err
		}

		// Step 2: Use pack pipeline with InitCommand (which internally uses FillCommand)
		initCmd := &packcommands.InitCommand{
			PackName: opts.PackName,
		}
		pipelineResult, err := orchestration.Execute(initCmd, []string{opts.PackName}, orchestration.Options{
			DotfilesRoot: opts.DotfilesRoot,
			DryRun:       opts.DryRun,
			FileSystem:   opts.FileSystem,
		})
		if err != nil {
			return nil, err
		}
		return convertPipelineResult(pipelineResult), nil

	case CommandFill:
		// Use pack pipeline for fill command
		fillCmd := &packcommands.FillCommand{}
		pipelineResult, err := orchestration.Execute(fillCmd, opts.PackNames, orchestration.Options{
			DotfilesRoot: opts.DotfilesRoot,
			DryRun:       opts.DryRun,
			FileSystem:   opts.FileSystem,
		})
		if err != nil {
			return nil, err
		}
		return convertPipelineResult(pipelineResult), nil

	case CommandAdopt:
		// Use pack pipeline for adopt command
		adoptCmd := &packcommands.AdoptCommand{
			SourcePaths: opts.SourcePaths,
			Force:       opts.Force,
		}
		pipelineResult, err := orchestration.Execute(adoptCmd, opts.PackNames, orchestration.Options{
			DotfilesRoot: opts.DotfilesRoot,
			DryRun:       opts.DryRun,
			FileSystem:   opts.FileSystem,
		})
		if err != nil {
			return nil, err
		}
		return convertPipelineResult(pipelineResult), nil

	case CommandAddIgnore:
		// Use pack pipeline for add-ignore command
		addIgnoreCmd := &packcommands.AddIgnoreCommand{}
		pipelineResult, err := orchestration.Execute(addIgnoreCmd, opts.PackNames, orchestration.Options{
			DotfilesRoot: opts.DotfilesRoot,
			DryRun:       opts.DryRun,
			FileSystem:   opts.FileSystem,
		})
		if err != nil {
			return nil, err
		}
		return convertPipelineResult(pipelineResult), nil

	default:
		return nil, fmt.Errorf("unknown command type: %s", cmdType)
	}
}

// convertPipelineResult converts pack pipeline result to types.PackCommandResult
func convertPipelineResult(pipelineResult *orchestration.Result) *display.PackCommandResult {
	result := &display.PackCommandResult{
		Command:   pipelineResult.Command,
		Timestamp: time.Now(),
		DryRun:    false,
		Packs:     make([]display.DisplayPack, 0, len(pipelineResult.PackResults)),
	}

	// Convert each pack result
	for _, pr := range pipelineResult.PackResults {
		// Extract status result if available
		if statusResult, ok := pr.CommandSpecificResult.(*packcommands.StatusResult); ok && statusResult != nil {
			displayPack := display.DisplayPack{
				Name:      statusResult.Name,
				HasConfig: statusResult.HasConfig,
				IsIgnored: statusResult.IsIgnored,
				Status:    statusResult.Status,
				Files:     make([]display.DisplayFile, 0, len(statusResult.Files)),
			}

			// Convert each file status
			for _, file := range statusResult.Files {
				displayFile := display.DisplayFile{
					Handler:        file.Handler,
					Path:           file.Path,
					Status:         statusStateToDisplayStatus(file.Status.State),
					Message:        file.Status.Message,
					LastExecuted:   file.Status.Timestamp,
					HandlerSymbol:  display.GetHandlerSymbol(file.Handler),
					AdditionalInfo: file.AdditionalInfo,
				}
				displayPack.Files = append(displayPack.Files, displayFile)
			}

			// Add special files if present
			if statusResult.IsIgnored {
				displayPack.Files = append([]display.DisplayFile{{
					Path:   ".dodotignore",
					Status: "ignored",
				}}, displayPack.Files...)
			}
			if statusResult.HasConfig {
				displayPack.Files = append([]display.DisplayFile{{
					Path:   ".dodot.toml",
					Status: "config",
				}}, displayPack.Files...)
			}

			result.Packs = append(result.Packs, displayPack)
		} else {
			// Fallback for commands that don't return status
			displayPack := display.DisplayPack{
				Name:   pr.Pack.Name,
				Status: "unknown",
			}
			if pr.Success {
				displayPack.Status = "success"
			} else if pr.Error != nil {
				displayPack.Status = "error"
			}
			result.Packs = append(result.Packs, displayPack)
		}
	}

	// Generate message based on command and results
	packNames := make([]string, 0, len(result.Packs))
	for _, pack := range result.Packs {
		packNames = append(packNames, pack.Name)
	}

	switch pipelineResult.Command {
	case "up":
		result.Message = display.FormatCommandMessage("turned on", packNames)
	case "down":
		result.Message = display.FormatCommandMessage("turned off", packNames)
	case "status":
		result.Message = "" // Status command doesn't have a message
	}

	return result
}

// statusStateToDisplayStatus converts internal status states to display status strings
func statusStateToDisplayStatus(state packcommands.StatusState) string {
	switch state {
	case packcommands.StatusStateReady, packcommands.StatusStateSuccess:
		return "success"
	case packcommands.StatusStateMissing:
		return "queue"
	case packcommands.StatusStatePending:
		return "queue"
	case packcommands.StatusStateError:
		return "error"
	case packcommands.StatusStateIgnored:
		return "ignored"
	case packcommands.StatusStateConfig:
		return "config"
	default:
		return "unknown"
	}
}
