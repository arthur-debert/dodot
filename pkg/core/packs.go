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
	logger.Trace().Str("root", dotfilesRoot).Msg("Getting pack candidates")

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
			logger.Trace().Str("name", name).Msg("Skipping hidden directory")
			continue
		}

		// Skip ignored patterns
		if shouldIgnore(name) {
			logger.Trace().Str("name", name).Msg("Skipping ignored pattern")
			continue
		}

		// Only consider directories
		if entry.IsDir() {
			fullPath := filepath.Join(dotfilesRoot, name)
			candidates = append(candidates, fullPath)
			logger.Trace().Str("path", fullPath).Msg("Found pack candidate")
		}
	}

	// Sort for consistent ordering
	sort.Strings(candidates)

	logger.Info().Int("count", len(candidates)).Msg("Found pack candidates")
	return candidates, nil
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

// GetPacks validates and creates Pack instances from candidate paths
func GetPacks(candidates []string) ([]types.Pack, error) {
	logger := logging.GetLogger("core.packs")
	logger.Trace().Int("count", len(candidates)).Msg("Validating pack candidates")

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

		// Skip packs with .dodotignore file
		if shouldIgnorePack(pack.Path) {
			logger.Info().
				Str("pack", pack.Name).
				Msg("Pack is skipped due to .dodotignore file")
			continue
		}

		packs = append(packs, pack)
		logger.Trace().
			Str("name", pack.Name).
			Str("path", pack.Path).
			Msg("Loaded pack")
	}

	// Sort packs by name for consistent ordering
	sort.Slice(packs, func(i, j int) bool {
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
		Metadata: make(map[string]interface{}),
	}

	// Load pack configuration if it exists
	configPath := filepath.Join(packPath, ".dodot.toml")
	if config.FileExists(configPath) {
		packConfig, err := loadPackConfig(configPath)
		if err != nil {
			return types.Pack{}, errors.Wrap(err, errors.ErrConfigLoad, "failed to load pack config").
				WithDetail("pack", packName).
				WithDetail("configPath", configPath)
		}
		pack.Config = packConfig
	}

	logger.Trace().
		Str("name", pack.Name).
		Bool("hasConfig", config.FileExists(configPath)).
		Msg("Pack loaded successfully")

	return pack, nil
}

// loadPackConfig reads and parses a pack's .dodot.toml configuration file
func loadPackConfig(configPath string) (types.PackConfig, error) {
	return config.LoadPackConfig(configPath)
}

// shouldIgnorePack checks if a pack should be ignored by checking for a .dodotignore file
func shouldIgnorePack(packPath string) bool {
	ignoreFilePath := filepath.Join(packPath, ".dodotignore")
	if _, err := os.Stat(ignoreFilePath); err == nil {
		return true
	}
	return false
}

// ValidatePack checks if a directory is a valid pack
func ValidatePack(packPath string) error {
	logger := logging.GetLogger("core.packs")

	// Check if path exists
	info, err := os.Stat(packPath)
	if err != nil {
		if os.IsNotExist(err) {
			return errors.Wrap(err, errors.ErrNotFound, "pack directory does not exist").
				WithDetail("path", packPath)
		}
		return errors.Wrap(err, errors.ErrFileAccess, "cannot access pack directory").
			WithDetail("path", packPath)
	}

	// Check if it's a directory
	if !info.IsDir() {
		return errors.New(errors.ErrPackInvalid, "pack path is not a directory").
			WithDetail("path", packPath)
	}

	// Check if it has a .dodot.toml with skip=true
	configPath := filepath.Join(packPath, ".dodot.toml")
	if config.FileExists(configPath) {
		_, err := loadPackConfig(configPath)
		if err != nil {
			// Config exists but is invalid
			return errors.Wrap(err, errors.ErrConfigLoad, "invalid pack configuration").
				WithDetail("path", packPath).
				WithDetail("configPath", configPath)
		}
	}

	// Check if directory is empty
	entries, err := os.ReadDir(packPath)
	if err != nil {
		return errors.Wrap(err, errors.ErrFileAccess, "cannot read pack directory").
			WithDetail("path", packPath)
	}

	// An empty directory is not a valid pack
	if len(entries) == 0 {
		return errors.New(errors.ErrPackEmpty, "pack directory is empty").
			WithDetail("path", packPath)
	}

	logger.Trace().Str("path", packPath).Msg("Pack validation successful")
	return nil
}

// SelectPacks filters a list of packs by name
func SelectPacks(allPacks []types.Pack, selectedNames []string) ([]types.Pack, error) {
	logger := logging.GetLogger("core.packs")

	if len(selectedNames) == 0 {
		// No selection means all packs
		return allPacks, nil
	}

	// Create a map for quick lookup
	packMap := make(map[string]types.Pack)
	for _, pack := range allPacks {
		packMap[pack.Name] = pack
	}

	var selected []types.Pack
	var notFound []string

	for _, name := range selectedNames {
		if pack, exists := packMap[name]; exists {
			selected = append(selected, pack)
			logger.Trace().Str("name", name).Msg("Selected pack")
		} else {
			notFound = append(notFound, name)
		}
	}

	if len(notFound) > 0 {
		return nil, errors.New(errors.ErrPackNotFound, "pack(s) not found").
			WithDetail("notFound", notFound).
			WithDetail("available", getPackNames(allPacks))
	}

	// Sort by name for consistent ordering
	sort.Slice(selected, func(i, j int) bool {
		return selected[i].Name < selected[j].Name
	})

	logger.Info().
		Int("selected", len(selected)).
		Int("total", len(allPacks)).
		Msg("Selected packs")

	return selected, nil
}

// getPackNames returns a list of pack names
func getPackNames(packs []types.Pack) []string {
	names := make([]string, len(packs))
	for i, pack := range packs {
		names[i] = pack.Name
	}
	return names
}
