package initialize

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/commands/fill"
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
	// FileSystem is the filesystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
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

	// 2. Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// 3. Build pack path and check if it exists
	pathsManager, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to initialize paths")
	}
	packPath := pathsManager.PackPath(opts.PackName)

	// Check if pack already exists
	if _, err := fs.Stat(packPath); err == nil {
		return nil, errors.Newf(errors.ErrPackExists, "pack %q already exists", opts.PackName)
	}

	// 4. Create the pack directory
	cfg := config.Default()
	log.Info().Str("directory", packPath).Msg("Creating pack directory")
	if err := fs.MkdirAll(packPath, os.FileMode(cfg.FilePermissions.Directory)); err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create pack directory")
	}

	var filesCreated []string

	// 5. Create pack configuration file
	// Get the default config content and comment it out
	configContent := config.GetUserDefaultsContent()
	commentedConfig := commentOutConfigValues(configContent)

	configPath := filepath.Join(packPath, ".dodot.toml")
	log.Info().Str("file", ".dodot.toml").Msg("Creating configuration file")
	if err := fs.WriteFile(configPath, []byte(commentedConfig), 0644); err != nil {
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
- README.txt      - This file

The following template files will be created based on your configuration:
- Shell configuration files (aliases, profile, etc.)
- Installation script (if needed)
- Homebrew dependencies file (if needed)
- PATH modifications (if needed)

Getting Started:
1. Edit .dodot.toml to customize handler mappings if needed
2. Run 'dodot fill ` + opts.PackName + `' to create template files
3. Add your dotfiles to this directory
4. Edit the template files to add your configurations
5. Run 'dodot link ` + opts.PackName + `' to deploy this pack

For more information, see: https://github.com/arthur-debert/dodot
`

	readmePath := filepath.Join(packPath, "README.txt")
	log.Info().Str("file", "README.txt").Msg("Creating README file")
	if err := fs.WriteFile(readmePath, []byte(readmeContent), os.FileMode(cfg.FilePermissions.File)); err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create README file")
	}
	filesCreated = append(filesCreated, "README.txt")

	// 7. Use fill command to create template files
	log.Info().Msg("Creating template files using fill command")
	fillOpts := fill.FillPackOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackName:     opts.PackName,
		FileSystem:   fs,
	}

	fillResult, err := fill.FillPack(fillOpts)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to fill pack with template files")
	}

	// Add the files created by fill command
	filesCreated = append(filesCreated, fillResult.FilesCreated...)

	// 8. Return result
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

// commentOutConfigValues takes the TOML content and comments out all non-comment, non-blank lines
// that contain configuration values (assignments)
func commentOutConfigValues(content string) string {
	lines := strings.Split(content, "\n")
	var result []string

	for _, line := range lines {
		trimmed := strings.TrimSpace(line)

		// Keep blank lines as-is
		if trimmed == "" {
			result = append(result, line)
			continue
		}

		// Keep lines that are already comments
		if strings.HasPrefix(trimmed, "#") {
			result = append(result, line)
			continue
		}

		// Keep section headers (e.g., [pack], [symlink]) as-is
		if strings.HasPrefix(trimmed, "[") && strings.HasSuffix(trimmed, "]") {
			result = append(result, line)
			continue
		}

		// Comment out configuration value lines
		result = append(result, "# "+line)
	}

	return strings.Join(result, "\n")
}
