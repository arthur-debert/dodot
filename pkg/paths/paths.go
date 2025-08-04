// Package paths provides centralized path handling for dodot.
// It implements XDG Base Directory specification compliance and
// provides a consistent API for all path operations in the codebase.
package paths

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/adrg/xdg"
	"github.com/arthur-debert/dodot/pkg/errors"
)

// Environment variable names
const (
	// EnvDotfilesRoot is the primary environment variable for dotfiles location
	EnvDotfilesRoot = "DOTFILES_ROOT"

	// EnvDodotDataDir overrides the XDG data directory for dodot
	EnvDodotDataDir = "DODOT_DATA_DIR"

	// EnvDodotConfigDir overrides the XDG config directory for dodot
	EnvDodotConfigDir = "DODOT_CONFIG_DIR"

	// EnvDodotCacheDir overrides the XDG cache directory for dodot
	EnvDodotCacheDir = "DODOT_CACHE_DIR"

	// EnvHome is the standard home directory variable
	EnvHome = "HOME"
)

// Default directories and files
const (
	// DefaultDotfilesDir is the default directory name for dotfiles
	DefaultDotfilesDir = "dotfiles"

	// DodotDirName is the directory name for dodot-specific files
	DodotDirName = "dodot"

	// PackConfigFile is the name of the pack configuration file
	PackConfigFile = ".dodot.toml"

	// StateDir is the subdirectory for state files
	StateDir = "state"

	// BackupsDir is the subdirectory for backups
	BackupsDir = "backups"

	// TemplatesDir is the subdirectory for templates
	TemplatesDir = "templates"

	// DeployedDir is the subdirectory for deployed files
	DeployedDir = "deployed"

	// ShellDir is the subdirectory for shell scripts
	ShellDir = "shell"

	// InstallDir is the subdirectory for install sentinels
	InstallDir = "install"

	// BrewfileDir is the subdirectory for brewfile sentinels
	BrewfileDir = "brewfile"

	// InitScriptName is the name of the init script
	InitScriptName = "dodot-init.sh"

	// LogFileName is the name of the log file
	LogFileName = "dodot.log"
)

// Paths provides centralized path management for dodot
type Paths struct {
	// dotfilesRoot is the root directory for all dotfiles
	dotfilesRoot string

	// xdgData is the XDG data directory
	xdgData string

	// xdgConfig is the XDG config directory
	xdgConfig string

	// xdgCache is the XDG cache directory
	xdgCache string

	// xdgState is the XDG state directory
	xdgState string

	// usedFallback indicates if we fell back to cwd (for warning display)
	usedFallback bool
}

// New creates a new Paths instance with the given dotfiles root.
// If dotfilesRoot is empty, it will be determined from environment variables
// or defaults.
func New(dotfilesRoot string) (*Paths, error) {
	p := &Paths{}

	// Set up dotfiles root
	if dotfilesRoot == "" {
		root, usedFallback, err := findDotfilesRoot()
		if err != nil {
			return nil, err
		}
		p.dotfilesRoot = root
		p.usedFallback = usedFallback
	} else {
		p.dotfilesRoot = expandHome(dotfilesRoot)
		p.usedFallback = false
	}

	// Ensure dotfiles root is absolute
	absRoot, err := filepath.Abs(p.dotfilesRoot)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrFileAccess, "failed to get absolute path for dotfiles root")
	}
	p.dotfilesRoot = absRoot

	// Set up XDG directories
	if err := p.setupXDGDirs(); err != nil {
		return nil, err
	}

	return p, nil
}

// setupXDGDirs initializes XDG directories, respecting environment overrides
func (p *Paths) setupXDGDirs() error {
	// Data directory
	if dataDir := os.Getenv(EnvDodotDataDir); dataDir != "" {
		p.xdgData = expandHome(dataDir)
	} else {
		p.xdgData = filepath.Join(xdg.DataHome, DodotDirName)
	}

	// Config directory
	if configDir := os.Getenv(EnvDodotConfigDir); configDir != "" {
		p.xdgConfig = expandHome(configDir)
	} else {
		p.xdgConfig = filepath.Join(xdg.ConfigHome, DodotDirName)
	}

	// Cache directory
	if cacheDir := os.Getenv(EnvDodotCacheDir); cacheDir != "" {
		p.xdgCache = expandHome(cacheDir)
	} else {
		p.xdgCache = filepath.Join(xdg.CacheHome, DodotDirName)
	}

	// State directory - XDG doesn't provide StateHome, so we check manually
	if stateDir := os.Getenv("XDG_STATE_HOME"); stateDir != "" {
		p.xdgState = filepath.Join(stateDir, DodotDirName)
	} else {
		homeDir, _ := os.UserHomeDir()
		p.xdgState = filepath.Join(homeDir, ".local", "state", DodotDirName)
	}

	return nil
}

// findDotfilesRoot determines the dotfiles root from environment or defaults
// It returns the path and a boolean indicating if fallback to cwd was used
func findDotfilesRoot() (string, bool, error) {
	// Check DOTFILES_ROOT first (highest priority)
	if root := os.Getenv(EnvDotfilesRoot); root != "" {
		return expandHome(root), false, nil
	}

	// Try to find git repository root
	gitRoot, err := findGitRoot()
	if err == nil && gitRoot != "" {
		if os.Getenv("DODOT_DEBUG") != "" {
			fmt.Fprintf(os.Stderr, "Debug: findDotfilesRoot using git root: %s\n", gitRoot)
		}
		return gitRoot, false, nil
	}

	// Fallback to current working directory with warning
	cwd, err := os.Getwd()
	if err != nil {
		return "", false, errors.Wrapf(err, errors.ErrFileAccess, "failed to get current directory")
	}

	return cwd, true, nil
}

// findGitRoot attempts to find the root of the current git repository
func findGitRoot() (string, error) {
	cmd := exec.Command("git", "rev-parse", "--show-toplevel")

	// Debug environment
	if os.Getenv("DODOT_DEBUG") != "" {
		cwd, _ := os.Getwd()
		fmt.Fprintf(os.Stderr, "Debug: findGitRoot called from: %s\n", cwd)
	}

	output, err := cmd.Output()
	if err != nil {
		// Git command failed - not in a git repo or git not installed
		if os.Getenv("DODOT_DEBUG") != "" {
			fmt.Fprintf(os.Stderr, "Debug: git command failed: %v\n", err)
		}
		return "", err
	}

	// Trim whitespace and return the path
	gitRoot := strings.TrimSpace(string(output))
	if gitRoot == "" {
		return "", errors.New(errors.ErrNotFound, "git root is empty")
	}

	if os.Getenv("DODOT_DEBUG") != "" {
		fmt.Fprintf(os.Stderr, "Debug: git root found: %s\n", gitRoot)
	}

	return gitRoot, nil
}

// expandHome expands ~ to the home directory
func expandHome(path string) string {
	if path == "" {
		return path
	}

	if path[0] == '~' {
		homeDir, err := os.UserHomeDir()
		if err != nil {
			// Fallback to HOME env var
			homeDir = os.Getenv(EnvHome)
			if homeDir == "" {
				// Can't expand, return as-is
				return path
			}
		}

		if len(path) == 1 {
			return homeDir
		}

		// Handle both ~/ and ~
		if path[1] == '/' || path[1] == filepath.Separator {
			return filepath.Join(homeDir, path[2:])
		}

		// ~something (not the user's home)
		return path
	}

	return path
}

// DotfilesRoot returns the root directory for dotfiles
func (p *Paths) DotfilesRoot() string {
	return p.dotfilesRoot
}

// UsedFallback returns true if the current working directory was used as fallback
func (p *Paths) UsedFallback() bool {
	return p.usedFallback
}

// PackPath returns the path to a specific pack
func (p *Paths) PackPath(packName string) string {
	return filepath.Join(p.dotfilesRoot, packName)
}

// PackConfigPath returns the path to a pack's configuration file
func (p *Paths) PackConfigPath(packName string) string {
	return filepath.Join(p.PackPath(packName), PackConfigFile)
}

// DataDir returns the XDG data directory for dodot
func (p *Paths) DataDir() string {
	return p.xdgData
}

// ConfigDir returns the XDG config directory for dodot
func (p *Paths) ConfigDir() string {
	return p.xdgConfig
}

// CacheDir returns the XDG cache directory for dodot
func (p *Paths) CacheDir() string {
	return p.xdgCache
}

// GetDataSubdir returns a subdirectory path under the XDG data directory.
// This is a helper method to reduce boilerplate for the many data subdirectories.
func (p *Paths) GetDataSubdir(name string) string {
	return filepath.Join(p.xdgData, name)
}

// StateDir returns the directory for state files
func (p *Paths) StateDir() string {
	return p.GetDataSubdir(StateDir)
}

// BackupsDir returns the directory for backup files
func (p *Paths) BackupsDir() string {
	return p.GetDataSubdir(BackupsDir)
}

// TemplatesDir returns the directory for template files
func (p *Paths) TemplatesDir() string {
	return p.GetDataSubdir(TemplatesDir)
}

// StatePath returns the path to a state file for a specific pack and powerup
func (p *Paths) StatePath(packName, powerUpName string) string {
	return filepath.Join(p.StateDir(), packName, powerUpName+".json")
}

// NormalizePath normalizes a path by expanding home, making it absolute,
// and cleaning it
func (p *Paths) NormalizePath(path string) (string, error) {
	if path == "" {
		return "", errors.New(errors.ErrInvalidInput, "empty path")
	}

	// Expand home directory
	expanded := expandHome(path)

	// Make absolute
	abs, err := filepath.Abs(expanded)
	if err != nil {
		return "", errors.Wrapf(err, errors.ErrFileAccess, "failed to get absolute path")
	}

	// Clean the path
	return filepath.Clean(abs), nil
}

// IsInDotfiles checks if a path is within the dotfiles root
func (p *Paths) IsInDotfiles(path string) (bool, error) {
	normalized, err := p.NormalizePath(path)
	if err != nil {
		return false, err
	}

	rel, err := filepath.Rel(p.dotfilesRoot, normalized)
	if err != nil {
		return false, nil
	}

	// If the relative path starts with .., it's outside dotfiles
	return !strings.HasPrefix(rel, ".."), nil
}

// ExpandHome is a utility function that expands ~ in paths
// This is exposed for compatibility with existing code
func ExpandHome(path string) string {
	return expandHome(path)
}

// DeployedDir returns the deployed directory path
func (p *Paths) DeployedDir() string {
	return p.GetDataSubdir(DeployedDir)
}

// GetDeployedSubdir returns a subdirectory path under the deployed directory.
// This is a helper method to reduce boilerplate for the deployed subdirectories.
func (p *Paths) GetDeployedSubdir(name string) string {
	return filepath.Join(p.DeployedDir(), name)
}

// ShellProfileDir returns the shell profile deployment directory
func (p *Paths) ShellProfileDir() string {
	return p.GetDeployedSubdir("shell_profile")
}

// PathDir returns the PATH deployment directory
func (p *Paths) PathDir() string {
	return p.GetDeployedSubdir("path")
}

// ShellSourceDir returns the shell source deployment directory
func (p *Paths) ShellSourceDir() string {
	return p.GetDeployedSubdir("shell_source")
}

// SymlinkDir returns the symlink deployment directory
func (p *Paths) SymlinkDir() string {
	return p.GetDeployedSubdir("symlink")
}

// ShellDir returns the shell scripts directory
func (p *Paths) ShellDir() string {
	return p.GetDataSubdir(ShellDir)
}

// InitScriptPath returns the path to the dodot-init.sh script
func (p *Paths) InitScriptPath() string {
	return filepath.Join(p.ShellDir(), InitScriptName)
}

// InstallDir returns the install scripts sentinel directory
func (p *Paths) InstallDir() string {
	return p.GetDataSubdir(InstallDir)
}

// BrewfileDir returns the brewfile sentinel directory
func (p *Paths) BrewfileDir() string {
	return p.GetDataSubdir(BrewfileDir)
}

// GetHomeDirectory returns the user's home directory with proper error handling
// This is migrated from pkg/utils/home.go
func GetHomeDirectory() (string, error) {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		// Try the HOME environment variable as a fallback
		if home := os.Getenv(EnvHome); home != "" {
			return home, nil
		}
		return "", errors.Wrapf(err, errors.ErrFileAccess, "failed to get home directory")
	}
	return homeDir, nil
}

// GetHomeDirectoryWithDefault returns the home directory or a default value
// This is migrated from pkg/utils/home.go
func GetHomeDirectoryWithDefault(defaultDir string) string {
	homeDir, err := GetHomeDirectory()
	if err != nil {
		return defaultDir
	}
	return homeDir
}

// LogFilePath returns the path to the dodot log file
// Respects XDG_STATE_HOME if set
func (p *Paths) LogFilePath() string {
	return filepath.Join(p.xdgState, LogFileName)
}
