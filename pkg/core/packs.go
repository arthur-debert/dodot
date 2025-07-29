package core

import (
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog/log"
)

// DefaultIgnorePatterns contains patterns for directories to ignore
var DefaultIgnorePatterns = []string{
	".git",
	".svn",
	".hg",
	"node_modules",
	".DS_Store",
	"*.swp",
	"*~",
	"#*#",
}

// GetPackCandidates returns all potential pack directories in the dotfiles root
func GetPackCandidates(dotfilesRoot string) ([]string, error) {
	logger := logging.GetLogger("core.packs")
	logger.Debug().Str("root", dotfilesRoot).Msg("Getting pack candidates")

	// Validate dotfiles root exists
	info, err := os.Stat(dotfilesRoot)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, errors.Wrap(err, errors.ErrNotFound, "dotfiles root does not exist").
				WithDetail("path", dotfilesRoot)
		}
		return nil, errors.Wrap(err, errors.ErrFileAccess, "cannot access dotfiles root").
			WithDetail("path", dotfilesRoot)
	}

	if !info.IsDir() {
		return nil, errors.New(errors.ErrInvalidInput, "dotfiles root is not a directory").
			WithDetail("path", dotfilesRoot)
	}

	// Read directory entries
	entries, err := os.ReadDir(dotfilesRoot)
	if err != nil {
		return nil, errors.Wrap(err, errors.ErrFileAccess, "cannot read dotfiles root").
			WithDetail("path", dotfilesRoot)
	}

	var candidates []string
	for _, entry := range entries {
		name := entry.Name()
		
		// Skip hidden directories (except .config which is common)
		if strings.HasPrefix(name, ".") && name != ".config" {
			logger.Debug().Str("name", name).Msg("Skipping hidden directory")
			continue
		}

		// Skip ignored patterns
		if shouldIgnore(name) {
			logger.Debug().Str("name", name).Msg("Skipping ignored pattern")
			continue
		}

		// Only consider directories
		if entry.IsDir() {
			fullPath := filepath.Join(dotfilesRoot, name)
			candidates = append(candidates, fullPath)
			logger.Debug().Str("path", fullPath).Msg("Found pack candidate")
		}
	}

	// Sort for consistent ordering
	sort.Strings(candidates)
	
	logger.Info().Int("count", len(candidates)).Msg("Found pack candidates")
	return candidates, nil
}

// GetPacks validates and creates Pack instances from candidate paths
func GetPacks(candidates []string) ([]types.Pack, error) {
	logger := logging.GetLogger("core.packs")
	logger.Debug().Int("count", len(candidates)).Msg("Validating pack candidates")

	var packs []types.Pack
	
	for _, candidatePath := range candidates {
		pack, err := loadPack(candidatePath)
		if err != nil {
			// Log the error but continue with other packs
			logger.Warn().
				Err(err).
				Str("path", candidatePath).
				Msg("Failed to load pack, skipping")
			continue
		}

		// Skip disabled packs
		if pack.Config.Disabled {
			logger.Info().
				Str("pack", pack.Name).
				Msg("Pack is disabled, skipping")
			continue
		}

		packs = append(packs, pack)
		logger.Debug().
			Str("name", pack.Name).
			Str("path", pack.Path).
			Int("priority", pack.Priority).
			Msg("Loaded pack")
	}

	// Sort packs by priority (descending) and then by name
	sort.Slice(packs, func(i, j int) bool {
		if packs[i].Priority != packs[j].Priority {
			return packs[i].Priority > packs[j].Priority
		}
		return packs[i].Name < packs[j].Name
	})

	logger.Info().Int("count", len(packs)).Msg("Loaded packs")
	return packs, nil
}

// loadPack creates a Pack instance from a directory path
func loadPack(packPath string) (types.Pack, error) {
	logger := log.With().Str("path", packPath).Logger()
	
	// Verify the path exists and is accessible
	info, err := os.Stat(packPath)
	if err != nil {
		return types.Pack{}, errors.Wrap(err, errors.ErrPackAccess, "cannot access pack directory").
			WithDetail("path", packPath)
	}

	if !info.IsDir() {
		return types.Pack{}, errors.New(errors.ErrPackInvalid, "pack path is not a directory").
			WithDetail("path", packPath)
	}

	// Extract pack name from directory
	packName := filepath.Base(packPath)
	
	// Create base pack
	pack := types.Pack{
		Name:     packName,
		Path:     packPath,
		Priority: 0, // Default priority
		Metadata: make(map[string]interface{}),
	}

	// Load pack configuration if it exists
	configPath := filepath.Join(packPath, ".dodot.toml")
	if config.FileExists(configPath) {
		config, err := loadPackConfig(configPath)
		if err != nil {
			return types.Pack{}, errors.Wrap(err, errors.ErrConfigLoad, "failed to load pack config").
				WithDetail("pack", packName).
				WithDetail("configPath", configPath)
		}
		pack.Config = config
		
		// Apply overrides from config
		if config.Description != "" {
			pack.Description = config.Description
		}
		if config.Priority != 0 {
			pack.Priority = config.Priority
		}
	}

	logger.Debug().
		Str("name", pack.Name).
		Bool("hasConfig", config.FileExists(configPath)).
		Msg("Pack loaded successfully")

	return pack, nil
}

// loadPackConfig reads and parses a pack's .dodot.toml configuration file
func loadPackConfig(configPath string) (types.PackConfig, error) {
	return config.LoadPackConfig(configPath)
}

// shouldIgnore checks if a name matches any ignore pattern
func shouldIgnore(name string) bool {
	for _, pattern := range DefaultIgnorePatterns {
		// Simple pattern matching (could be enhanced with glob)
		if matched, _ := filepath.Match(pattern, name); matched {
			return true
		}
	}
	return false
}

