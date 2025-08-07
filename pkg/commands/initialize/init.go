package initialize

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// InitPackOptions defines the options for the InitPack command.
type InitPackOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackName is the name of the new pack to create.
	PackName string
}

// InitPack creates a new pack directory with template files and configuration.
func InitPack(opts InitPackOptions) (*types.InitResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "InitPack").Str("pack", opts.PackName).Msg("Executing command")

	// 1. Validate pack name
	if opts.PackName == "" {
		return nil, errors.New(errors.ErrInvalidInput, "pack name cannot be empty")
	}

	// Check for invalid characters in pack name
	if strings.ContainsAny(opts.PackName, "/\\:*?\"<>|") {
		return nil, errors.Newf(errors.ErrInvalidInput, "pack name contains invalid characters: %s", opts.PackName)
	}

	// 2. Build pack path and check if it exists
	packPath := filepath.Join(opts.DotfilesRoot, opts.PackName)

	// Check if pack already exists
	if _, err := os.Stat(packPath); err == nil {
		return nil, errors.Newf(errors.ErrPackExists, "pack %q already exists", opts.PackName)
	}

	// 3. Generate actions for creating pack
	cfg := config.Default()
	var actions []types.Action

	// Create directory action
	mkdirAction := types.Action{
		Type:        types.ActionTypeMkdir,
		Description: fmt.Sprintf("Create pack directory %s", opts.PackName),
		Target:      packPath,
		Mode:        uint32(cfg.FilePermissions.Directory),
		Pack:        opts.PackName,
		PowerUpName: "init_pack_internal",
		Priority:    200, // Higher priority to create dir first
	}
	actions = append(actions, mkdirAction)

	// Create .dodot.toml configuration file
	configContent := `# dodot configuration for ` + opts.PackName + ` pack
# See https://github.com/arthur-debert/dodot for documentation

# Uncomment to skip this pack during deployment
# skip = true

# File-specific rules
[files]
# Ignore specific files
# "*.bak" = "ignore"
# "*.tmp" = "ignore"

# Override default power-up for specific files
# "my-script.sh" = "install"
# "my-aliases.sh" = "profile"
`

	configPath := filepath.Join(packPath, ".dodot.toml")
	configAction := types.Action{
		Type:        types.ActionTypeWrite,
		Description: "Create .dodot.toml configuration",
		Target:      configPath,
		Content:     configContent,
		Mode:        uint32(cfg.FilePermissions.File),
		Pack:        opts.PackName,
		PowerUpName: "init_pack_internal",
		Priority:    100,
	}
	actions = append(actions, configAction)

	// 4. Create README.txt
	readmeContent := `dodot Pack: ` + opts.PackName + `
====================

This pack was created by dodot init. It contains configuration files and scripts
for the ` + opts.PackName + ` environment.

Files in this pack:
- .dodot.toml     - Pack configuration
- aliases.sh      - Shell aliases (sourced in shell profile)
- install.sh      - Installation script (runs once during 'dodot install')
- Brewfile        - Homebrew dependencies (processed during 'dodot install')
- path.sh         - PATH modifications (sourced in shell profile)
- README.txt      - This file

Getting Started:
1. Add your dotfiles to this directory
2. Edit the template files to add your configurations
3. Run 'dodot deploy ` + opts.PackName + `' to deploy this pack

For more information, see: https://github.com/arthur-debert/dodot
`

	readmePath := filepath.Join(packPath, "README.txt")
	readmeAction := types.Action{
		Type:        types.ActionTypeWrite,
		Description: "Create README.txt",
		Target:      readmePath,
		Content:     readmeContent,
		Mode:        uint32(cfg.FilePermissions.File),
		Pack:        opts.PackName,
		PowerUpName: "init_pack_internal",
		Priority:    100,
	}
	actions = append(actions, readmeAction)

	// 5. Get all template files for the pack
	templates, err := core.GetCompletePackTemplate(opts.PackName)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get pack templates")
	}

	// Add template file actions
	for _, template := range templates {
		action := types.Action{
			Type:        types.ActionTypeWrite,
			Description: fmt.Sprintf("Create template file %s", template.Filename),
			Target:      filepath.Join(packPath, template.Filename),
			Content:     template.Content,
			Mode:        template.Mode,
			Pack:        opts.PackName,
			PowerUpName: template.PowerUpName,
			Priority:    50, // Lower priority for template files
		}
		actions = append(actions, action)
	}

	// 6. Execute actions using Executor (Operations no longer returned)
	if len(actions) > 0 {
		// Initialize paths
		pathsInstance, err := paths.New(opts.DotfilesRoot)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to initialize paths")
		}

		// Create Executor
		directExecutorOpts := &core.ExecutorOptions{
			Paths:             pathsInstance,
			DryRun:            false,
			Force:             true,
			AllowHomeSymlinks: false,
			Config:            config.Default(),
		}

		executor := core.NewExecutor(directExecutorOpts)

		// Execute actions and extract operations from results
		results, err := executor.ExecuteActions(actions)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrActionExecute, "failed to execute init actions")
		}

		// FIXME: ARCHITECTURAL PROBLEM - init command should return Pack+PowerUp+File information
		// NOT operation details. See docs/design/display.txxt
		// Operations are no longer returned (part of Operation layer elimination)
		_ = results // Results processed but not exposed in return value
	}

	// 7. Return result (Operations field removed as part of Operation elimination)
	result := &types.InitResult{
		PackName:     opts.PackName,
		Path:         packPath,
		FilesCreated: []string{},
	}

	// Report all files that would be created
	for _, action := range actions {
		switch action.Type {
		case types.ActionTypeMkdir:
			log.Info().
				Str("directory", action.Target).
				Str("operation", "would create").
				Msg("Pack directory to be created")
		case types.ActionTypeWrite:
			filename := filepath.Base(action.Target)
			result.FilesCreated = append(result.FilesCreated, filename)
			log.Info().
				Str("file", filename).
				Str("operation", "would create").
				Msg("File to be created")
		}
	}

	log.Debug().
		Int("actionCount", len(actions)).
		Msg("Executed actions for InitPack")

	log.Info().Str("command", "InitPack").
		Str("pack", opts.PackName).
		Str("path", packPath).
		Int("filesCreated", len(result.FilesCreated)).
		Msg("Command finished")
	return result, nil
}
