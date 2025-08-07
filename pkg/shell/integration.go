package shell

import (
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/rs/zerolog/log"
)

// InstallShellIntegration installs shell integration scripts to the user's data directory
func InstallShellIntegration(dataDir string) error {
	shellDir := filepath.Join(dataDir, "shell")

	// Create shell directory
	if err := os.MkdirAll(shellDir, 0755); err != nil {
		return fmt.Errorf("failed to create shell directory: %w", err)
	}

	// Scripts to install
	scripts := []string{"dodot-init.sh", "dodot-init.fish"}

	for _, script := range scripts {
		// Find source script
		sourcePath, err := paths.ResolveShellScriptPath(script)
		if err != nil {
			log.Warn().Str("script", script).Msg("Shell script not found, skipping")
			continue
		}

		// Open source file
		source, err := os.Open(sourcePath)
		if err != nil {
			return fmt.Errorf("failed to open source script %s: %w", script, err)
		}
		defer func() { _ = source.Close() }()

		// Create destination file
		destPath := filepath.Join(shellDir, script)
		dest, err := os.Create(destPath)
		if err != nil {
			return fmt.Errorf("failed to create destination script %s: %w", script, err)
		}
		defer func() { _ = dest.Close() }()

		// Copy content
		if _, err := io.Copy(dest, source); err != nil {
			return fmt.Errorf("failed to copy script %s: %w", script, err)
		}

		// Make executable
		if err := os.Chmod(destPath, 0755); err != nil {
			return fmt.Errorf("failed to make script executable: %w", err)
		}

		log.Info().Str("script", script).Str("dest", destPath).Msg("Installed shell integration script")
	}

	return nil
}