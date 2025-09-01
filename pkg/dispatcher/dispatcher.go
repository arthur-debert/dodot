// Package dispatcher provides centralized command dispatching for pack operations.
// It acts as the entry point from the CLI layer, eliminating the need for
// individual command packages.
package dispatcher

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packcommands"
	"github.com/arthur-debert/dodot/pkg/packpipeline"
	"github.com/arthur-debert/dodot/pkg/packpipeline/commands"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// CommandType represents the type of pack command being executed
type CommandType string

const (
	// Core commands
	CommandOn     CommandType = "on"
	CommandOff    CommandType = "off"
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
func Dispatch(cmdType CommandType, opts Options) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("dispatcher")
	logger.Debug().
		Str("command", string(cmdType)).
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Msg("Dispatching pack command")

	var result *types.PackCommandResult
	var err error

	switch cmdType {
	case CommandOn:
		// Use pack pipeline for on command
		onCmd := &commands.OnCommand{
			NoProvision: opts.NoProvision,
			Force:       opts.Force,
		}
		pipelineResult, err := packpipeline.Execute(onCmd, opts.PackNames, packpipeline.Options{
			DotfilesRoot: opts.DotfilesRoot,
			DryRun:       opts.DryRun,
			FileSystem:   opts.FileSystem,
		})
		if err != nil {
			return nil, err
		}
		return convertPipelineResult(pipelineResult), nil

	case CommandOff:
		// Use pack pipeline for off command
		offCmd := &commands.OffCommand{}
		pipelineResult, err := packpipeline.Execute(offCmd, opts.PackNames, packpipeline.Options{
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
		statusCmd := &commands.StatusCommand{}
		pipelineResult, err := packpipeline.Execute(statusCmd, opts.PackNames, packpipeline.Options{
			DotfilesRoot: opts.DotfilesRoot,
			DryRun:       false, // Status is always a query
			FileSystem:   opts.FileSystem,
		})
		if err != nil {
			return nil, err
		}
		return convertPipelineResult(pipelineResult), nil

	case CommandInit:
		// For init, we need to create a status function
		getPackStatus := func(packName, dotfilesRoot string, fs types.FS) ([]types.DisplayPack, error) {
			statusResult, err := packcommands.GetPacksStatus(packcommands.StatusCommandOptions{
				DotfilesRoot: dotfilesRoot,
				PackNames:    []string{packName},
				FileSystem:   fs,
			})
			if err != nil {
				return nil, err
			}
			return statusResult.Packs, nil
		}

		result, err = packcommands.Initialize(packcommands.InitOptions{
			PackName:      opts.PackName,
			DotfilesRoot:  opts.DotfilesRoot,
			FileSystem:    opts.FileSystem,
			GetPackStatus: getPackStatus,
		})

	case CommandFill:
		// Extract pack name for fill command
		var packName string
		if len(opts.PackNames) > 0 {
			packName = opts.PackNames[0]
		}

		result, err = packcommands.Fill(packcommands.FillOptions{
			PackName:     packName,
			DotfilesRoot: opts.DotfilesRoot,
			FileSystem:   opts.FileSystem,
		})

	case CommandAdopt:
		// Extract pack name for adopt command
		var packName string
		if len(opts.PackNames) > 0 {
			packName = opts.PackNames[0]
		}

		// Skip pack validation here - it will be handled by packcommands.Adopt
		// This avoids circular dependency issues

		// Create status function
		getPackStatus := func(packName, dotfilesRoot string, fs types.FS) ([]types.DisplayPack, error) {
			statusResult, err := packcommands.GetPacksStatus(packcommands.StatusCommandOptions{
				DotfilesRoot: dotfilesRoot,
				PackNames:    []string{packName},
				FileSystem:   fs,
			})
			if err != nil {
				return nil, err
			}
			return statusResult.Packs, nil
		}

		result, err = packcommands.Adopt(packcommands.AdoptOptions{
			SourcePaths:   opts.SourcePaths,
			Force:         opts.Force,
			DotfilesRoot:  opts.DotfilesRoot,
			PackName:      packName,
			FileSystem:    opts.FileSystem,
			GetPackStatus: getPackStatus,
		})

	case CommandAddIgnore:
		// Extract pack name for add-ignore command
		var packName string
		if len(opts.PackNames) > 0 {
			packName = opts.PackNames[0]
		}

		// Create status function
		getPackStatus := func(packName, dotfilesRoot string, fs types.FS) ([]types.DisplayPack, error) {
			statusResult, err := packcommands.GetPacksStatus(packcommands.StatusCommandOptions{
				DotfilesRoot: dotfilesRoot,
				PackNames:    []string{packName},
				FileSystem:   fs,
			})
			if err != nil {
				return nil, err
			}
			return statusResult.Packs, nil
		}

		result, err = packcommands.AddIgnore(packcommands.AddIgnoreOptions{
			PackName:      packName,
			DotfilesRoot:  opts.DotfilesRoot,
			FileSystem:    opts.FileSystem,
			GetPackStatus: getPackStatus,
		})

	default:
		return nil, fmt.Errorf("unknown command type: %s", cmdType)
	}

	if err != nil {
		logger.Error().
			Str("command", string(cmdType)).
			Err(err).
			Msg("Command execution failed")
		return nil, err
	}

	logger.Info().
		Str("command", string(cmdType)).
		Int("packCount", len(result.Packs)).
		Msg("Command completed successfully")

	return result, nil
}

// convertPipelineResult converts pack pipeline result to types.PackCommandResult
func convertPipelineResult(pipelineResult *packpipeline.Result) *types.PackCommandResult {
	result := &types.PackCommandResult{
		Command:   pipelineResult.Command,
		Timestamp: time.Now(),
		DryRun:    false,
		Packs:     make([]types.DisplayPack, 0, len(pipelineResult.PackResults)),
	}

	// Convert each pack result
	for _, pr := range pipelineResult.PackResults {
		// Extract status result if available
		if statusResult, ok := pr.CommandSpecificResult.(*packcommands.StatusResult); ok && statusResult != nil {
			displayPack := types.DisplayPack{
				Name:      statusResult.Name,
				HasConfig: statusResult.HasConfig,
				IsIgnored: statusResult.IsIgnored,
				Status:    statusResult.Status,
				Files:     make([]types.DisplayFile, 0, len(statusResult.Files)),
			}

			// Convert each file status
			for _, file := range statusResult.Files {
				displayFile := types.DisplayFile{
					Handler:        file.Handler,
					Path:           file.Path,
					Status:         statusStateToDisplayStatus(file.Status.State),
					Message:        file.Status.Message,
					LastExecuted:   file.Status.Timestamp,
					HandlerSymbol:  types.GetHandlerSymbol(file.Handler),
					AdditionalInfo: file.AdditionalInfo,
				}
				displayPack.Files = append(displayPack.Files, displayFile)
			}

			// Add special files if present
			if statusResult.IsIgnored {
				displayPack.Files = append([]types.DisplayFile{{
					Path:   ".dodotignore",
					Status: "ignored",
				}}, displayPack.Files...)
			}
			if statusResult.HasConfig {
				displayPack.Files = append([]types.DisplayFile{{
					Path:   ".dodot.toml",
					Status: "config",
				}}, displayPack.Files...)
			}

			result.Packs = append(result.Packs, displayPack)
		} else {
			// Fallback for commands that don't return status
			displayPack := types.DisplayPack{
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
	case "on":
		result.Message = types.FormatCommandMessage("turned on", packNames)
	case "off":
		result.Message = types.FormatCommandMessage("turned off", packNames)
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
