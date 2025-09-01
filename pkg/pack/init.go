package pack

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/statustype"
	"github.com/arthur-debert/dodot/pkg/types"
)

// InitOptions contains options for the Initialize operation
type InitOptions struct {
	// PackName is the name of the pack to create
	PackName string
	// DotfilesRoot is the root directory for dotfiles
	DotfilesRoot string
	// FileSystem is the filesystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
	// GetPackStatus is a function to get pack status to avoid circular imports
	GetPackStatus statustype.GetPackStatusFunc
}

// Initialize creates a new pack with the standard structure and template files.
// This is a static method since we're creating a new pack, not operating on an existing one.
func Initialize(opts InitOptions) (*types.PackCommandResult, error) {
	log := logging.GetLogger("pack.init")
	log.Debug().Str("pack", opts.PackName).Msg("Initializing new pack")

	// Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Validate pack name
	if opts.PackName == "" {
		return nil, errors.New(errors.ErrInvalidInput, "pack name cannot be empty")
	}

	// Check for invalid characters in pack name
	if strings.ContainsAny(opts.PackName, "/\\:*?\"<>|") {
		return nil, errors.Newf(errors.ErrInvalidInput, "pack name contains invalid characters: %s", opts.PackName)
	}

	// Build pack path
	packPath := filepath.Join(opts.DotfilesRoot, opts.PackName)

	// Check if pack already exists
	if _, err := fs.Stat(packPath); err == nil {
		return nil, errors.Newf(errors.ErrPackExists, "pack %q already exists", opts.PackName)
	}

	// Create the pack directory
	cfg := config.Default()
	log.Info().Str("directory", packPath).Msg("Creating pack directory")
	if err := fs.MkdirAll(packPath, os.FileMode(cfg.FilePermissions.Directory)); err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create pack directory")
	}

	var filesCreated []string

	// Create pack configuration file
	configContent := config.GetUserDefaultsContent()
	commentedConfig := commentOutConfigValues(configContent)

	configPath := filepath.Join(packPath, ".dodot.toml")
	log.Info().Str("file", ".dodot.toml").Msg("Creating configuration file")
	if err := fs.WriteFile(configPath, []byte(commentedConfig), 0644); err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create configuration file")
	}
	filesCreated = append(filesCreated, ".dodot.toml")

	// Create README.txt
	readmeContent := generateReadmeContent(opts.PackName)
	readmePath := filepath.Join(packPath, "README.txt")
	log.Info().Str("file", "README.txt").Msg("Creating README file")
	if err := fs.WriteFile(readmePath, []byte(readmeContent), os.FileMode(cfg.FilePermissions.File)); err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create README file")
	}
	filesCreated = append(filesCreated, "README.txt")

	// Now we need to create a Pack instance to use Fill
	// First, load the configuration we just wrote
	packConfig := config.PackConfig{} // Use default config for new pack
	p := &types.Pack{
		Name:   opts.PackName,
		Path:   packPath,
		Config: packConfig,
	}

	// Wrap in our enhanced Pack type and fill with template files
	enhancedPack := New(p)
	log.Info().Msg("Creating template files")
	fillResult, err := enhancedPack.Fill(fs)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to fill pack with template files")
	}

	// Add the files created by fill
	filesCreated = append(filesCreated, fillResult.FilesCreated...)

	log.Info().
		Str("pack", opts.PackName).
		Str("path", packPath).
		Int("filesCreated", len(filesCreated)).
		Msg("Pack initialization completed")

	// Get current pack status if function provided
	var packStatus []types.DisplayPack
	if opts.GetPackStatus != nil {
		var statusErr error
		packStatus, statusErr = opts.GetPackStatus(opts.PackName, opts.DotfilesRoot, fs)
		if statusErr != nil {
			log.Error().Err(statusErr).Msg("Failed to get pack status")
		}
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "init",
		Timestamp: time.Now(),
		DryRun:    false,
		Packs:     []types.DisplayPack{},
		Metadata: types.CommandMetadata{
			FilesCreated: len(filesCreated),
			CreatedPaths: filesCreated,
		},
	}

	// Copy packs from status if available
	if packStatus != nil {
		result.Packs = packStatus
	}

	// Generate message
	result.Message = fmt.Sprintf("The pack %s has been initialized with %d files.", opts.PackName, len(filesCreated))

	return result, nil
}

// generateReadmeContent creates the README content for a new pack
func generateReadmeContent(packName string) string {
	return `dodot Pack: ` + packName + `
====================

This pack was created by dodot init. It contains configuration files and scripts
for the ` + packName + ` environment.

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
2. Run 'dodot fill ` + packName + `' to create template files
3. Add your dotfiles to this directory
4. Edit the template files to add your configurations
5. Run 'dodot link ` + packName + `' to deploy this pack

For more information, see: https://github.com/arthur-debert/dodot
`
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
