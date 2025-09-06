package paths

import (
	"os"
	"path/filepath"

	"github.com/rs/zerolog/log"
)

// ResolveShellScriptPath finds the shell integration script with fallback logic
// It first tries the installed location, then falls back to development location
func ResolveShellScriptPath(scriptName string) (string, error) {
	// Try development location first if PROJECT_ROOT is set
	// This ensures tests and development work correctly
	if projectRoot := os.Getenv("PROJECT_ROOT"); projectRoot != "" {
		devPath := filepath.Join(projectRoot, "pkg", "shell", scriptName)
		if _, err := os.Stat(devPath); err == nil {
			log.Debug().Str("path", devPath).Str("script", scriptName).Msg("Found shell script in development location")
			return devPath, nil
		}
	}

	// Try installed location (relative to binary)
	exePath, err := os.Executable()
	if err == nil {
		// Look for shell scripts in various installed locations
		installedPaths := []string{
			// Homebrew installs to pkgshare: /opt/homebrew/share/dodot/shell/
			"/opt/homebrew/share/dodot/shell/" + scriptName,
			"/usr/local/share/dodot/shell/" + scriptName,                                      // Intel Mac homebrew
			filepath.Join(filepath.Dir(exePath), "..", "share", "dodot", "shell", scriptName), // Standard Unix layout
			filepath.Join(filepath.Dir(exePath), "..", "share", "shell", scriptName),          // Alternative Unix layout
			filepath.Join(filepath.Dir(exePath), "shell", scriptName),                         // Same directory as binary
			filepath.Join(filepath.Dir(exePath), "..", "shell", scriptName),                   // Parent directory
		}

		for _, path := range installedPaths {
			if _, err := os.Stat(path); err == nil {
				log.Debug().Str("path", path).Str("script", scriptName).Msg("Found shell script in installed location")
				return path, nil
			}
		}
	}

	// Try development location using PROJECT_ROOT
	if projectRoot := os.Getenv("PROJECT_ROOT"); projectRoot != "" {
		devPath := filepath.Join(projectRoot, "pkg", "shell", scriptName)
		if _, err := os.Stat(devPath); err == nil {
			log.Debug().Str("path", devPath).Str("script", scriptName).Msg("Found shell script in development location")
			return devPath, nil
		}
	}

	// Try relative to current working directory as last resort
	cwd, err := os.Getwd()
	if err == nil {
		cwdPath := filepath.Join(cwd, "pkg", "shell", scriptName)
		if _, err := os.Stat(cwdPath); err == nil {
			log.Debug().Str("path", cwdPath).Str("script", scriptName).Msg("Found shell script relative to cwd")
			return cwdPath, nil
		}
	}

	return "", os.ErrNotExist
}

// GetShellScriptPath returns the path to the shell integration script
// It uses the resolved path if found, otherwise returns the expected installation path
func GetShellScriptPath(shell string) string {
	var scriptName string
	switch shell {
	case "fish":
		scriptName = "dodot-init.fish"
	default:
		scriptName = "dodot-init.sh"
	}

	// Try to resolve the actual path
	if path, err := ResolveShellScriptPath(scriptName); err == nil {
		return path
	}

	// Fallback to expected location in user's data directory
	// This is what the snippet will reference
	dataDir := os.Getenv("DODOT_DATA_DIR")
	if dataDir == "" {
		if xdgData := os.Getenv("XDG_DATA_HOME"); xdgData != "" {
			dataDir = filepath.Join(xdgData, "dodot")
		} else {
			dataDir = filepath.Join(os.Getenv("HOME"), ".local", "share", "dodot")
		}
	}

	return filepath.Join(dataDir, "shell", scriptName)
}
