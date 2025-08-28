package initialize

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
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
// templateFile holds basic template information
type templateFile struct {
	filename    string
	content     string
	mode        uint32
	handlerName string
}

// getBasicTemplates returns basic template files for pack initialization
func getBasicTemplates(packName string) []templateFile {
	return []templateFile{
		{
			filename:    "aliases.sh",
			content:     "# Shell aliases for " + packName + " pack\n# Add your aliases below\n\n# Example:\n# alias ll='ls -la'\n",
			mode:        0644,
			handlerName: "shell",
		},
		{
			filename:    "install.sh",
			content:     "#!/usr/bin/env bash\n# Installation script for " + packName + " pack\n# This script runs once during 'dodot install'\n\necho \"Installing " + packName + " pack...\"\n\n# Add your installation commands here\n",
			mode:        0755,
			handlerName: "provision",
		},
		{
			filename:    "Brewfile",
			content:     "# Homebrew dependencies for " + packName + " pack\n# This file is processed during 'dodot install'\n\n# Add your brew dependencies:\n# brew \"git\"\n# cask \"visual-studio-code\"\n",
			mode:        0644,
			handlerName: "homebrew",
		},
		{
			filename:    "path.sh",
			content:     "# PATH modifications for " + packName + " pack\n# Add directories to PATH below\n\n# Example:\n# export PATH=\"$HOME/.local/bin:$PATH\"\n",
			mode:        0644,
			handlerName: "path",
		},
	}
}

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
	pathsManager, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to initialize paths")
	}
	packPath := pathsManager.PackPath(opts.PackName)

	// Check if pack already exists
	if _, err := os.Stat(packPath); err == nil {
		return nil, errors.Newf(errors.ErrPackExists, "pack %q already exists", opts.PackName)
	}

	// 3. Create filesystem instance for file operations
	fs := filesystem.NewOS()
	cfg := config.Default()

	// 4. Create the pack directory
	log.Info().Str("directory", packPath).Msg("Creating pack directory")
	if err := fs.MkdirAll(packPath, os.FileMode(cfg.FilePermissions.Directory)); err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create pack directory")
	}

	var filesCreated []string

	// 5. Create .dodot.toml configuration file
	configContent := `# dodot configuration for ` + opts.PackName + ` pack
# See https://github.com/arthur-debert/dodot for documentation

# Uncomment to skip this pack during deployment
# skip = true

# File-specific rules
[files]
# Ignore specific files
# "*.bak" = "ignore"
# "*.tmp" = "ignore"

# Override default handler for specific files
# "my-script.sh" = "provision"
# "my-aliases.sh" = "profile"
`

	configPath := pathsManager.PackConfigPath(opts.PackName)
	log.Info().Str("file", ".dodot.toml").Msg("Creating configuration file")
	if err := fs.WriteFile(configPath, []byte(configContent), os.FileMode(cfg.FilePermissions.File)); err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create configuration file")
	}
	filesCreated = append(filesCreated, ".dodot.toml")

	// 6. Create README.txt
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
	log.Info().Str("file", "README.txt").Msg("Creating README file")
	if err := fs.WriteFile(readmePath, []byte(readmeContent), os.FileMode(cfg.FilePermissions.File)); err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create README file")
	}
	filesCreated = append(filesCreated, "README.txt")

	// 7. Get all template files for the pack
	// TODO: Re-implement using rules system
	// For now, create basic template files manually
	templates := getBasicTemplates(opts.PackName)

	// 8. Create each template file
	for _, template := range templates {
		templatePath := filepath.Join(packPath, template.filename)
		log.Info().Str("file", template.filename).Str("handler", template.handlerName).Msg("Creating template file")
		if err := fs.WriteFile(templatePath, []byte(template.content), os.FileMode(template.mode)); err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create template file %s", template.filename)
		}
		filesCreated = append(filesCreated, template.filename)
	}

	// 9. Return result
	result := &types.InitResult{
		PackName:     opts.PackName,
		Path:         packPath,
		FilesCreated: filesCreated,
	}

	log.Debug().
		Int("filesCreated", len(filesCreated)).
		Msg("Created pack files")

	log.Info().Str("command", "InitPack").
		Str("pack", opts.PackName).
		Str("path", packPath).
		Int("filesCreated", len(result.FilesCreated)).
		Msg("Command finished")
	return result, nil
}
