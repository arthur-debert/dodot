// Package core provides core functionality for the dodot CLI
package core

import (
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/shell"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OutputType represents the type of output being generated
type OutputType string

const (
	// OutputTypeConfig generates configuration file content
	OutputTypeConfig OutputType = "config"
	// OutputTypeSnippet generates shell integration snippets
	OutputTypeSnippet OutputType = "snippet"
)

// OutputOptions contains options for the output command
type OutputOptions struct {
	// Type specifies what kind of output to generate
	Type OutputType
	// Write indicates whether to write the output to files
	Write bool
	// FileSystem is the filesystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
	// Additional options specific to the output type
	Config  *ConfigOutputOptions
	Snippet *SnippetOutputOptions
}

// ConfigOutputOptions contains options specific to config output
type ConfigOutputOptions struct {
	DotfilesRoot string
	PackNames    []string
}

// SnippetOutputOptions contains options specific to snippet output
type SnippetOutputOptions struct {
	Shell     string
	DataDir   string
	Provision bool // Install shell integration scripts
}

// OutputResult contains the result of an output operation
type OutputResult struct {
	// Content is the generated content
	Content string
	// FilesWritten contains paths of files that were written (if Write was true)
	FilesWritten []string
	// Metadata contains type-specific metadata
	Metadata map[string]interface{}
}

// GenerateOutput generates text output based on the specified type and options
func GenerateOutput(opts OutputOptions) (*OutputResult, error) {
	logger := logging.GetLogger("core.output")
	logger.Debug().
		Str("type", string(opts.Type)).
		Bool("write", opts.Write).
		Msg("Generating output")

	// Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	var content string
	var err error
	var metadata map[string]interface{}

	// Generate content based on type
	switch opts.Type {
	case OutputTypeConfig:
		if opts.Config == nil {
			return nil, fmt.Errorf("config options required for config output")
		}
		content, err = generateConfigContent()
		if err != nil {
			return nil, fmt.Errorf("failed to generate config content: %w", err)
		}
		metadata = map[string]interface{}{
			"type": "config",
		}

	case OutputTypeSnippet:
		if opts.Snippet == nil {
			return nil, fmt.Errorf("snippet options required for snippet output")
		}
		content, metadata, err = generateSnippetContent(opts.Snippet, fs)
		if err != nil {
			return nil, fmt.Errorf("failed to generate snippet content: %w", err)
		}

	default:
		return nil, fmt.Errorf("unknown output type: %s", opts.Type)
	}

	result := &OutputResult{
		Content:      content,
		FilesWritten: []string{},
		Metadata:     metadata,
	}

	// If not writing, just return the content
	if !opts.Write {
		logger.Debug().Msg("Outputting to stdout")
		return result, nil
	}

	// Handle file writing based on type
	switch opts.Type {
	case OutputTypeConfig:
		filesWritten, err := writeConfigFiles(fs, content, opts.Config)
		if err != nil {
			return result, err
		}
		result.FilesWritten = filesWritten

	case OutputTypeSnippet:
		// Snippets are not written to files directly
		// The provision flag is handled in generateSnippetContent
	}

	return result, nil
}

// generateConfigContent generates the configuration file content
func generateConfigContent() (string, error) {
	return config.GenerateConfigContent(), nil
}

// generateSnippetContent generates shell integration snippet content
func generateSnippetContent(opts *SnippetOutputOptions, fs types.FS) (string, map[string]interface{}, error) {
	logger := logging.GetLogger("core.output")
	metadata := make(map[string]interface{})

	// Handle provisioning if requested
	var installMessage string
	if opts.Provision {
		if err := shell.InstallShellIntegration(opts.DataDir); err != nil {
			return "", nil, fmt.Errorf("failed to install shell integration: %w", err)
		}
		installMessage = fmt.Sprintf("Shell integration scripts installed to %s/shell/", opts.DataDir)
		logger.Info().Str("dataDir", opts.DataDir).Msg("Installed shell integration scripts")
	}

	// Get the appropriate snippet for the shell
	snippetText := shell.GetShellIntegrationSnippet(opts.Shell, opts.DataDir)

	// Set metadata
	metadata["shell"] = opts.Shell
	metadata["dataDir"] = opts.DataDir
	metadata["installed"] = opts.Provision
	if installMessage != "" {
		metadata["installMessage"] = installMessage
	}

	return snippetText, metadata, nil
}

// writeConfigFiles writes configuration files to the specified locations
func writeConfigFiles(fs types.FS, content string, opts *ConfigOutputOptions) ([]string, error) {
	logger := logging.GetLogger("core.output")
	var filesWritten []string

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
			return filesWritten, fmt.Errorf("failed to create directory %s: %w", dir, err)
		}

		// Check if file already exists
		if _, err := fs.Stat(targetPath); err == nil {
			logger.Warn().Str("path", targetPath).Msg("Config file already exists, skipping")
			continue
		}

		// Write the file
		if err := fs.WriteFile(targetPath, []byte(content), 0644); err != nil {
			return filesWritten, fmt.Errorf("failed to write config to %s: %w", targetPath, err)
		}

		logger.Info().Str("path", targetPath).Msg("Written config file")
		filesWritten = append(filesWritten, targetPath)
	}

	return filesWritten, nil
}
