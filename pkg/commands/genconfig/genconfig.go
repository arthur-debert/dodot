package genconfig

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GenConfigOptions holds options for the gen-config command
type GenConfigOptions struct {
	DotfilesRoot string
	PackNames    []string
	Write        bool
}

// GenConfig outputs or writes the default configuration
func GenConfig(opts GenConfigOptions) (*types.GenConfigResult, error) {
	logger := logging.GetLogger("commands.genconfig")

	// Get the user defaults content from the config package
	userDefaultsContent := config.GetUserDefaultsContent()

	result := &types.GenConfigResult{
		ConfigContent: userDefaultsContent,
		FilesWritten:  []string{},
	}

	// If not writing, just return the content
	if !opts.Write {
		logger.Debug().Msg("Outputting config to stdout")
		return result, nil
	}

	// Write mode
	logger.Info().Bool("write", opts.Write).Msg("Writing config files")

	// Determine where to write files
	var targetPaths []string

	if len(opts.PackNames) == 0 {
		// No packs specified, write to current directory
		targetPaths = append(targetPaths, ".dodot.toml")
	} else {
		// Write to each specified pack
		for _, packName := range opts.PackNames {
			packPath := filepath.Join(opts.DotfilesRoot, packName)
			targetPath := filepath.Join(packPath, ".dodot.toml")
			targetPaths = append(targetPaths, targetPath)
		}
	}

	// Write files
	for _, targetPath := range targetPaths {
		// Ensure directory exists
		dir := filepath.Dir(targetPath)
		if err := os.MkdirAll(dir, 0755); err != nil {
			return result, fmt.Errorf("failed to create directory %s: %w", dir, err)
		}

		// Check if file already exists
		if _, err := os.Stat(targetPath); err == nil {
			logger.Warn().Str("path", targetPath).Msg("Config file already exists, skipping")
			continue
		}

		// Write the file
		if err := os.WriteFile(targetPath, []byte(userDefaultsContent), 0644); err != nil {
			return result, fmt.Errorf("failed to write config to %s: %w", targetPath, err)
		}

		logger.Info().Str("path", targetPath).Msg("Written config file")
		result.FilesWritten = append(result.FilesWritten, targetPath)
	}

	return result, nil
}
