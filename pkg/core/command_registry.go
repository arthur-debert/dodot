package core

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/display"
)

// CommandExecutionType defines whether a command uses simple file operations or handler execution
type CommandExecutionType string

const (
	// SimpleExecution for commands that perform direct file operations
	SimpleExecution CommandExecutionType = "simple"
	// HandlerExecution for commands that go through the handler pipeline
	HandlerExecution CommandExecutionType = "handler"
)

// CommandConfig defines the configuration for a registered command
type CommandConfig struct {
	// Name is the command identifier
	Name string
	// Type indicates if this is a simple or handler-based command
	Type CommandExecutionType
	// Execute is the function that performs the command logic
	Execute func(opts CommandExecuteOptions) (*display.PackCommandResult, error)
	// Validators are command-specific validation functions run before execution
	Validators []ValidatorFunc
}

// CommandExecuteOptions contains options for executing any command
type CommandExecuteOptions struct {
	// DotfilesRoot is the root directory containing packs
	DotfilesRoot string
	// PackNames specifies which packs to operate on
	PackNames []string
	// DryRun specifies whether to perform a dry run
	DryRun bool
	// Force forces operations even if there are conflicts
	Force bool
	// FileSystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS

	// Command-specific options
	Options map[string]interface{}
}

// ValidatorFunc is a function that validates command prerequisites
type ValidatorFunc func(packs []types.Pack, opts CommandExecuteOptions) error

// CommandRegistry maps command names to their configurations
var CommandRegistry = make(map[string]CommandConfig)

// RegisterCommand adds a command to the registry
func RegisterCommand(config CommandConfig) {
	CommandRegistry[config.Name] = config
}

// ExecuteRegisteredCommand runs a command from the registry with standard flow
func ExecuteRegisteredCommand(cmdName string, opts CommandExecuteOptions) (*display.PackCommandResult, error) {
	logger := logging.GetLogger("core.command_registry")
	logger.Debug().
		Str("command", cmdName).
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Msg("Executing registered command")

	// Look up command configuration
	config, exists := CommandRegistry[cmdName]
	if !exists {
		return nil, fmt.Errorf("unknown command: %s", cmdName)
	}

	// For handler-based commands, we may use different discovery
	var packs []types.Pack
	var err error

	// Special handling for commands that create packs (like init)
	if cmdName == "init" {
		// For init, we don't discover existing packs
		packs = make([]types.Pack, 0)
	} else {
		// Standard pack discovery for most commands
		packs, err = DiscoverAndSelectPacksFS(opts.DotfilesRoot, opts.PackNames, opts.FileSystem)
		if err != nil {
			logger.Error().Err(err).Msg("Failed to discover packs")
			// Special error message for adopt command
			if cmdName == "adopt" {
				return nil, fmt.Errorf("pack '%s' does not exist. Please use 'dodot init %s' to create it first", opts.PackNames[0], opts.PackNames[0])
			}
			return nil, fmt.Errorf("pack discovery failed: %w", err)
		}
	}

	// Run command-specific validators
	for _, validator := range config.Validators {
		if err := validator(packs, opts); err != nil {
			logger.Error().Err(err).Msg("Validation failed")
			return nil, err
		}
	}

	// Execute the command
	result, err := config.Execute(opts)
	if err != nil {
		logger.Error().Err(err).Msg("Command execution failed")
		return nil, err
	}

	logger.Info().
		Str("command", cmdName).
		Int("packCount", len(result.Packs)).
		Msg("Command completed successfully")

	return result, nil
}
