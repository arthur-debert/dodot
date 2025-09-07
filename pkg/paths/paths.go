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
	"github.com/arthur-debert/dodot/pkg/types"
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
// IMPORTANT: These constants define dodot's internal datastore structure and
// are NOT user-configurable. They must remain consistent across all dodot
// installations to ensure proper operation. User-configurable paths should
// be added to pkg/config instead.
const (
	// DefaultDotfilesDir is the default directory name for dotfiles
	DefaultDotfilesDir = "dotfiles"

	// DodotDirName is the directory name for dodot-specific files
	DodotDirName = "dodot"

	// PackConfigFile is the name of the pack configuration file
	PackConfigFile = ".dodot.toml"

	// StateDir is the subdirectory for state files
	StateDir = "state"

	// TemplatesDir is the subdirectory for templates
	TemplatesDir = "templates"

	// DeployedDir is the subdirectory for deployed files
	DeployedDir = "deployed"

	// ShellDir is the subdirectory for shell scripts
	ShellDir = "shell"

	// ProvisionDir is the subdirectory for provision sentinels
	ProvisionDir = "provision"

	// HomebrewDir is the subdirectory for homebrew sentinels
	HomebrewDir = "homebrew"

	// InitScriptName is the name of the init script
	InitScriptName = "dodot-init.sh"

	// LogFileName is the name of the log file
	LogFileName = "dodot.log"
)

// Paths provides centralized path management for dodot
type Paths interface {
	DotfilesRoot() string
	UsedFallback() bool
	PackPath(packName string) string
	PackConfigPath(packName string) string
	DataDir() string
	ConfigDir() string
	CacheDir() string
	StateDir() string
	TemplatesDir() string
	StatePath(packName, handlerName string) string
	NormalizePath(path string) (string, error)
	IsInDotfiles(path string) (bool, error)
	DeployedDir() string
	ShellProfileDir() string
	PathDir() string
	ShellSourceDir() string
	SymlinkDir() string
	ShellDir() string
	InitScriptPath() string
	ProvisionDir() string
	HomebrewDir() string
	SentinelPath(handlerType, packName string) string
	LogFilePath() string
	MapPackFileToSystem(pack *types.Pack, relPath string) string
	MapSystemFileToPack(pack *types.Pack, systemPath string) string
	PackHandlerDir(packName, handlerName string) string
}

// paths provides centralized path management for dodot
type paths struct {
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
func New(dotfilesRoot string) (Paths, error) {
	p := &paths{}

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
func (p *paths) setupXDGDirs() error {
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

// findDotfilesRoot determines the dotfiles root using the following priority:
// 1. DOTFILES_ROOT environment variable (if set)
// 2. Git repository root (found via 'git rev-parse --show-toplevel')
// 3. Current working directory (fallback)
//
// The function returns:
// - string: The resolved dotfiles root path
// - bool: Whether the current working directory was used as fallback
// - error: Any error that occurred during resolution
//
// This allows dodot to work in three common scenarios:
// - Explicit configuration via DOTFILES_ROOT
// - Automatic detection when run from within a git-managed dotfiles repo
// - Fallback to current directory for quick testing or non-git setups
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
func (p *paths) DotfilesRoot() string {
	return p.dotfilesRoot
}

// UsedFallback returns true if the current working directory was used as fallback
func (p *paths) UsedFallback() bool {
	return p.usedFallback
}

// PackPath returns the path to a specific pack
func (p *paths) PackPath(packName string) string {
	return filepath.Join(p.dotfilesRoot, packName)
}

// PackConfigPath returns the path to a pack's configuration file
func (p *paths) PackConfigPath(packName string) string {
	return filepath.Join(p.PackPath(packName), PackConfigFile)
}

// DataDir returns the XDG data directory for dodot
func (p *paths) DataDir() string {
	return p.xdgData
}

// ConfigDir returns the XDG config directory for dodot
func (p *paths) ConfigDir() string {
	return p.xdgConfig
}

// CacheDir returns the XDG cache directory for dodot
func (p *paths) CacheDir() string {
	return p.xdgCache
}

// GetDataSubdir returns a subdirectory path under the XDG data directory.
// This is a helper method to reduce boilerplate for the many data subdirectories.
func (p *paths) GetDataSubdir(name string) string {
	return filepath.Join(p.xdgData, name)
}

// StateDir returns the directory for state files
func (p *paths) StateDir() string {
	return p.GetDataSubdir(StateDir)
}

// TemplatesDir returns the directory for template files
func (p *paths) TemplatesDir() string {
	return p.GetDataSubdir(TemplatesDir)
}

// StatePath returns the path to a state file for a specific pack and handler
func (p *paths) StatePath(packName, handlerName string) string {
	return filepath.Join(p.StateDir(), packName, handlerName+".json")
}

// NormalizePath normalizes a path by expanding home, making it absolute,
// and cleaning it
func (p *paths) NormalizePath(path string) (string, error) {
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
func (p *paths) IsInDotfiles(path string) (bool, error) {
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
func (p *paths) DeployedDir() string {
	return p.GetDataSubdir(DeployedDir)
}

// GetDeployedSubdir returns a subdirectory path under the deployed directory.
// This is a helper method to reduce boilerplate for the deployed subdirectories.
func (p *paths) GetDeployedSubdir(name string) string {
	return filepath.Join(p.DeployedDir(), name)
}

// ShellProfileDir returns the shell profile deployment directory
func (p *paths) ShellProfileDir() string {
	return p.GetDeployedSubdir("shell")
}

// PathDir returns the PATH deployment directory
func (p *paths) PathDir() string {
	return p.GetDeployedSubdir("path")
}

// ShellSourceDir returns the shell source deployment directory
func (p *paths) ShellSourceDir() string {
	return p.GetDeployedSubdir("shell_source")
}

// SymlinkDir returns the symlink deployment directory
func (p *paths) SymlinkDir() string {
	return p.GetDeployedSubdir("symlink")
}

// ShellDir returns the shell scripts directory
func (p *paths) ShellDir() string {
	return p.GetDataSubdir(ShellDir)
}

// InitScriptPath returns the path to the dodot-init.sh script
func (p *paths) InitScriptPath() string {
	return filepath.Join(p.ShellDir(), InitScriptName)
}

// ProvisionDir returns the provision scripts sentinel directory
func (p *paths) ProvisionDir() string {
	return p.GetDataSubdir(ProvisionDir)
}

// HomebrewDir returns the homebrew sentinel directory
func (p *paths) HomebrewDir() string {
	return p.GetDataSubdir(HomebrewDir)
}

// SentinelPath returns the path to a sentinel file for a given handler and pack.
// This provides a unified way to construct sentinel file paths across the codebase.
// The sentinel file is used to track whether a run-once operation has been executed.
//
// The path structure is: <DataDir>/<handlerType>/<packName>
// For example: ~/.local/share/dodot/provision/vim
//
// Currently supported handlerTypes:
//   - "provision" - for install.sh scripts
//   - "homebrew" - for Brewfile executions
func (p *paths) SentinelPath(handlerType, packName string) string {
	switch handlerType {
	case "install":
		return filepath.Join(p.ProvisionDir(), "sentinels", packName)
	case "homebrew":
		return filepath.Join(p.HomebrewDir(), packName)
	default:
		// For future extensibility, we could use a generic sentinel directory
		// return filepath.Join(p.GetDataSubdir("sentinels"), handlerType, packName)
		// For now, we'll just return the same pattern as existing code
		return filepath.Join(p.GetDataSubdir(handlerType), packName)
	}
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
func (p *paths) LogFilePath() string {
	return filepath.Join(p.xdgState, LogFileName)
}

// isTopLevel checks if a file is at the pack root (no directory separators)
func isTopLevel(relPath string) bool {
	return !strings.Contains(relPath, string(filepath.Separator))
}

// stripDotPrefix removes a leading dot from a filename if present
func stripDotPrefix(filename string) string {
	if strings.HasPrefix(filename, ".") && len(filename) > 1 {
		return filename[1:]
	}
	return filename
}

// hasExplicitOverride checks for _home/ or _xdg/ prefix
// Returns true if an override is found, along with the override type ("home" or "xdg")
// Release D: Layer 3 - Explicit Overrides
func hasExplicitOverride(relPath string) (bool, string) {
	if strings.HasPrefix(relPath, "_home/") {
		return true, "home"
	}
	if strings.HasPrefix(relPath, "_xdg/") {
		return true, "xdg"
	}
	return false, ""
}

// stripOverridePrefix removes _home/ or _xdg/ from path
// Release D: Layer 3 - Explicit Overrides
func stripOverridePrefix(relPath string) string {
	if strings.HasPrefix(relPath, "_home/") {
		return strings.TrimPrefix(relPath, "_home/")
	}
	if strings.HasPrefix(relPath, "_xdg/") {
		return strings.TrimPrefix(relPath, "_xdg/")
	}
	return relPath
}

// getFirstSegment returns the first path segment

// MapPackFileToSystem maps a file from a pack to its deployment location.
// Priority order (highest to lowest):
// Layer 3: Explicit overrides (_home/ or _xdg/ prefix)
// Layer 2: Force home configuration (pack overrides root)
// Layer 1: Smart default mapping
func (p *paths) MapPackFileToSystem(pack *types.Pack, relPath string) string {
	// Get home directory first (used by multiple layers)
	homeDir, err := GetHomeDirectory()
	if err != nil {
		homeDir = "~" // Fallback for safety, though GetHomeDirectory is robust
	}

	// Layer 3: Check for explicit overrides (_home/ or _xdg/ prefix) - HIGHEST PRIORITY
	if hasOverride, overrideType := hasExplicitOverride(relPath); hasOverride {
		strippedPath := stripOverridePrefix(relPath)

		switch overrideType {
		case "home":
			// _home/ files always go to $HOME with dot prefix on the first segment
			parts := strings.Split(strippedPath, string(filepath.Separator))
			if len(parts) > 0 && parts[0] != "" && !strings.HasPrefix(parts[0], ".") {
				parts[0] = "." + parts[0]
			}
			return filepath.Join(homeDir, filepath.Join(parts...))
		case "xdg":
			// _xdg/ files always go to XDG_CONFIG_HOME
			xdgConfigHome := os.Getenv("XDG_CONFIG_HOME")
			if xdgConfigHome == "" {
				xdgConfigHome = filepath.Join(homeDir, ".config")
			}
			return filepath.Join(xdgConfigHome, strippedPath)
		}
	}

	// Layer 2: No force_home check here - that's a config concern
	// The caller should handle force_home logic if needed

	// Layer 1: Smart default mapping
	if isTopLevel(relPath) {
		// Top-level files go to $HOME with dot prefix
		filename := filepath.Base(relPath)
		// Add dot prefix if not already present
		if !strings.HasPrefix(filename, ".") {
			filename = "." + filename
		}
		return filepath.Join(homeDir, filename)
	}

	// Subdirectory files go to XDG_CONFIG_HOME
	xdgConfigHome := os.Getenv("XDG_CONFIG_HOME")
	if xdgConfigHome == "" {
		xdgConfigHome = filepath.Join(homeDir, ".config")
	}

	// Special case: if the file is already under .config or config in the pack,
	// strip that prefix to avoid .config/.config/... or .config/config/...
	relPath = strings.TrimPrefix(relPath, ".config/")
	relPath = strings.TrimPrefix(relPath, "config/")

	return filepath.Join(xdgConfigHome, relPath)
}

// MapSystemFileToPack determines where a system file should be placed in a pack.
// Release E: Updated to handle Layer 4 configuration file (with Layer 3, 2, and 1 fallback)
// Note: Layer 3 and Layer 4 reverse mapping is not automatic - users must manually
// organize files into _home/_xdg/ directories or configure mappings in .dodot.toml.
func (p *paths) MapSystemFileToPack(pack *types.Pack, systemPath string) string {
	// Get home directory
	homeDir, err := GetHomeDirectory()
	if err != nil {
		homeDir = filepath.Dir(systemPath) // Fallback
	}

	// Get XDG paths
	xdgConfigHome := os.Getenv("XDG_CONFIG_HOME")
	if xdgConfigHome == "" {
		xdgConfigHome = filepath.Join(homeDir, ".config")
	}

	// Get the base name of the file
	baseName := filepath.Base(systemPath)

	// Check if this is under HOME and potentially matches an exception
	if strings.HasPrefix(systemPath, homeDir) {
		relFromHome, err := filepath.Rel(homeDir, systemPath)
		if err == nil && strings.HasPrefix(relFromHome, ".") {
			// Get first segment to check against exceptions
			parts := strings.Split(relFromHome, string(filepath.Separator))
			if len(parts) > 0 {
				firstSegment := stripDotPrefix(parts[0])

				// Exception list items are stored without dot prefix
				// The caller should check force_home/exceptions if needed
				parts[0] = firstSegment
				return filepath.Join(pack.Path, filepath.Join(parts...))
			}
		}
	}

	// Layer 1 reverse mapping:
	// If file is directly in $HOME (dotfile), it goes to pack root without dot
	if filepath.Dir(systemPath) == homeDir {
		// Remove leading dot for pack organization
		packName := stripDotPrefix(baseName)
		return filepath.Join(pack.Path, packName)
	}

	// If file is in XDG config path, preserve full directory structure
	if strings.HasPrefix(systemPath, xdgConfigHome) {
		// Get relative path from XDG_CONFIG_HOME
		relPath, err := filepath.Rel(xdgConfigHome, systemPath)
		if err == nil {
			// If the pack already has files under .config/, maintain that structure
			// This is a heuristic - in future releases we might handle this differently
			return filepath.Join(pack.Path, relPath)
		}
	}

	// For files in hidden directories under HOME, extract subdirectory structure
	// Example: ~/.ssh/config -> ssh/config (note: ssh without dot)
	if strings.HasPrefix(systemPath, homeDir) {
		relFromHome, err := filepath.Rel(homeDir, systemPath)
		if err == nil && strings.HasPrefix(relFromHome, ".") {
			// Split the path and find the hidden directory
			parts := strings.Split(relFromHome, string(filepath.Separator))
			if len(parts) > 0 && strings.HasPrefix(parts[0], ".") {
				// Remove the dot from the first part
				parts[0] = stripDotPrefix(parts[0])
				return filepath.Join(pack.Path, filepath.Join(parts...))
			}
		}
	}

	// For other paths with hidden directories, preserve structure after hidden dir
	// This maintains backward compatibility with existing behavior
	if strings.Contains(systemPath, "/.") {
		parts := strings.Split(systemPath, string(filepath.Separator))
		for i, part := range parts {
			if strings.HasPrefix(part, ".") && part != "." && i < len(parts)-1 {
				// Found a hidden directory, use everything after it
				subPath := filepath.Join(parts[i+1:]...)
				return filepath.Join(pack.Path, subPath)
			}
		}
	}

	// Default: use base name without dot prefix
	packName := stripDotPrefix(baseName)
	return filepath.Join(pack.Path, packName)
}

func (p *paths) PackHandlerDir(packName, handlerName string) string {
	return filepath.Join(p.DataDir(), "packs", packName, handlerName)
}
