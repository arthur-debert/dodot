package commands

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/pack"
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

// DispatchOptions contains all possible options for pack commands.
// Each command will use only the fields it needs.
type DispatchOptions struct {
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
func Dispatch(cmdType CommandType, opts DispatchOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("commands.dispatch")
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
		result, err = pack.TurnOn(pack.OnOptions{
			DotfilesRoot:   opts.DotfilesRoot,
			PackNames:      opts.PackNames,
			DryRun:         opts.DryRun,
			Force:          opts.Force,
			NoProvision:    opts.NoProvision,
			ProvisionRerun: opts.ProvisionRerun,
			FileSystem:     opts.FileSystem,
		})

	case CommandOff:
		result, err = pack.TurnOff(pack.OffOptions{
			DotfilesRoot: opts.DotfilesRoot,
			PackNames:    opts.PackNames,
			DryRun:       opts.DryRun,
			FileSystem:   opts.FileSystem,
		})

	case CommandStatus:
		result, err = pack.GetPacksStatus(pack.StatusCommandOptions{
			DotfilesRoot: opts.DotfilesRoot,
			PackNames:    opts.PackNames,
			Paths:        opts.Paths,
			FileSystem:   opts.FileSystem,
		})

	case CommandInit:
		// For init, we need to create a status function
		getPackStatus := func(packName, dotfilesRoot string, fs types.FS) ([]types.DisplayPack, error) {
			statusResult, err := pack.GetPacksStatus(pack.StatusCommandOptions{
				DotfilesRoot: dotfilesRoot,
				PackNames:    []string{packName},
				FileSystem:   fs,
			})
			if err != nil {
				return nil, err
			}
			return statusResult.Packs, nil
		}

		result, err = pack.Initialize(pack.InitOptions{
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

		result, err = pack.Fill(pack.FillOptions{
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

		// Skip pack validation here - it will be handled by pack.Adopt
		// This avoids circular dependency issues

		// Create status function
		getPackStatus := func(packName, dotfilesRoot string, fs types.FS) ([]types.DisplayPack, error) {
			statusResult, err := pack.GetPacksStatus(pack.StatusCommandOptions{
				DotfilesRoot: dotfilesRoot,
				PackNames:    []string{packName},
				FileSystem:   fs,
			})
			if err != nil {
				return nil, err
			}
			return statusResult.Packs, nil
		}

		result, err = pack.Adopt(pack.AdoptOptions{
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
			statusResult, err := pack.GetPacksStatus(pack.StatusCommandOptions{
				DotfilesRoot: dotfilesRoot,
				PackNames:    []string{packName},
				FileSystem:   fs,
			})
			if err != nil {
				return nil, err
			}
			return statusResult.Packs, nil
		}

		result, err = pack.AddIgnore(pack.AddIgnoreOptions{
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
