package genconfig

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GenConfigOptions holds options for the gen-config command
type GenConfigOptions struct {
	DotfilesRoot string
	PackNames    []string
	Write        bool
	FileSystem   types.FS // Optional filesystem for testing
}

// GenConfig outputs or writes the default configuration
func GenConfig(opts GenConfigOptions) (*types.GenConfigResult, error) {
	logger := logging.GetLogger("commands.genconfig")

	// Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Get the user defaults content from the config package
	userDefaultsContent := config.GetUserDefaultsContent()

	// Comment out the configuration values
	commentedContent := commentOutConfigValues(userDefaultsContent)

	result := &types.GenConfigResult{
		ConfigContent: commentedContent,
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
		if err := fs.MkdirAll(dir, 0755); err != nil {
			return result, fmt.Errorf("failed to create directory %s: %w", dir, err)
		}

		// Check if file already exists
		if _, err := fs.Stat(targetPath); err == nil {
			logger.Warn().Str("path", targetPath).Msg("Config file already exists, skipping")
			continue
		}

		// Write the file
		if err := fs.WriteFile(targetPath, []byte(result.ConfigContent), 0644); err != nil {
			return result, fmt.Errorf("failed to write config to %s: %w", targetPath, err)
		}

		logger.Info().Str("path", targetPath).Msg("Written config file")
		result.FilesWritten = append(result.FilesWritten, targetPath)
	}

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
