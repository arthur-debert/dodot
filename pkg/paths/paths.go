// Package paths provides centralized path handling for dodot.
// It implements XDG Base Directory specification compliance and
// provides a consistent API for all path operations in the codebase.
package paths

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/adrg/xdg"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/rs/zerolog"
)

// Environment variable names
const (
	// EnvDotfilesRoot is the primary environment variable for dotfiles location
	EnvDotfilesRoot = "DOTFILES_ROOT"
	
	// EnvDotfilesHome is the legacy environment variable (deprecated)
	EnvDotfilesHome = "DOTFILES_HOME"
	
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
	
	log zerolog.Logger
}

// New creates a new Paths instance with the given dotfiles root.
// If dotfilesRoot is empty, it will be determined from environment variables
// or defaults.
func New(dotfilesRoot string) (*Paths, error) {
	log := logging.GetLogger("paths")
	
	p := &Paths{
		log: log,
	}
	
	// Set up dotfiles root
	if dotfilesRoot == "" {
		root, err := findDotfilesRoot()
		if err != nil {
			return nil, err
		}
		p.dotfilesRoot = root
	} else {
		p.dotfilesRoot = expandHome(dotfilesRoot)
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
	
	log.Debug().
		Str("dotfilesRoot", p.dotfilesRoot).
		Str("xdgData", p.xdgData).
		Str("xdgConfig", p.xdgConfig).
		Str("xdgCache", p.xdgCache).
		Msg("Paths initialized")
	
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
	
	return nil
}

// findDotfilesRoot determines the dotfiles root from environment or defaults
func findDotfilesRoot() (string, error) {
	// Check DOTFILES_ROOT first
	if root := os.Getenv(EnvDotfilesRoot); root != "" {
		return expandHome(root), nil
	}
	
	// Check legacy DOTFILES_HOME
	if home := os.Getenv(EnvDotfilesHome); home != "" {
		log := logging.GetLogger("paths")
		log.Warn().
			Msg("DOTFILES_HOME is deprecated, please use DOTFILES_ROOT")
		return expandHome(home), nil
	}
	
	// Default to ~/dotfiles
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return "", errors.Wrapf(err, errors.ErrFileAccess, "failed to get home directory")
	}
	
	return filepath.Join(homeDir, DefaultDotfilesDir), nil
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

// StateDir returns the directory for state files
func (p *Paths) StateDir() string {
	return filepath.Join(p.xdgData, StateDir)
}

// BackupsDir returns the directory for backup files
func (p *Paths) BackupsDir() string {
	return filepath.Join(p.xdgData, BackupsDir)
}

// TemplatesDir returns the directory for template files
func (p *Paths) TemplatesDir() string {
	return filepath.Join(p.xdgData, TemplatesDir)
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