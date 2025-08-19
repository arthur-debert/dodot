package packs

import (
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	toml "github.com/pelletier/go-toml/v2"
	"github.com/rs/zerolog/log"
)

// GetPackCandidates returns all potential pack directories in the dotfiles root
// Deprecated: Use GetPackCandidatesFS instead to support filesystem abstraction
//
// IMPORTANT: Commands should NOT call this function directly. Instead, use
// the centralized helper core.DiscoverAndSelectPacks which properly handles
// pack discovery, loading, and selection in a consistent way.
func GetPackCandidates(dotfilesRoot string) ([]string, error) {
	logger := logging.GetLogger("packs.discovery")
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
	cfg := config.Default()
	return shouldIgnoreWithPatterns(name, cfg.Patterns.PackIgnore)
}

// shouldIgnoreWithPatterns is a testable version that accepts patterns as parameter
func shouldIgnoreWithPatterns(name string, patterns []string) bool {
	for _, pattern := range patterns {
		// Simple pattern matching (could be enhanced with glob)
		if matched, _ := filepath.Match(pattern, name); matched {
			return true
		}
	}
	return false
}

// GetPacks validates and creates Pack instances from candidate paths
// Deprecated: Use GetPacksFS instead to support filesystem abstraction
//
// IMPORTANT: Commands should NOT call this function directly. Instead, use
// the centralized helper core.DiscoverAndSelectPacks which properly handles
// pack discovery, loading, and selection in a consistent way.
func GetPacks(candidates []string) ([]types.Pack, error) {
	logger := logging.GetLogger("packs.discovery")
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
		if ShouldIgnorePack(pack.Path) {
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

	// Validate pack name
	if err := paths.ValidatePackName(packName); err != nil {
		logger.Warn().Str("pack", packName).Err(err).Msg("Skipping pack with invalid name")
		return types.Pack{}, errors.Wrap(err, errors.ErrPackInvalid, "invalid pack name").
			WithDetail("pack", packName).
			WithDetail("path", packPath)
	}

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
	return LoadPackConfig(configPath)
}

// ValidatePack checks if a directory is a valid pack
func ValidatePack(packPath string) error {
	logger := logging.GetLogger("packs.discovery")

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
	cfg := config.Default()
	configPath := filepath.Join(packPath, cfg.Patterns.SpecialFiles.PackConfig)
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

// GetPackCandidatesFS returns all potential pack directories using the provided filesystem
//
// IMPORTANT: Commands should NOT call this function directly. Instead, use
// the centralized helper core.DiscoverAndSelectPacks which properly handles
// pack discovery, loading, and selection in a consistent way.
func GetPackCandidatesFS(dotfilesRoot string, filesystem types.FS) ([]string, error) {
	logger := logging.GetLogger("packs.discovery")
	logger.Trace().Str("root", dotfilesRoot).Msg("Getting pack candidates with FS")

	// Validate dotfiles root exists
	info, err := filesystem.Stat(dotfilesRoot)
	if err != nil {
		return nil, errors.Wrap(err, errors.ErrNotFound, "dotfiles root does not exist").
			WithDetail("path", dotfilesRoot)
	}

	if !info.IsDir() {
		return nil, errors.New(errors.ErrInvalidInput, "dotfiles root is not a directory").
			WithDetail("path", dotfilesRoot)
	}

	// Now that types.FS includes ReadDir, we can use it directly
	entries, err := filesystem.ReadDir(dotfilesRoot)
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

// GetPacksFS validates and creates Pack instances from candidate paths using the provided filesystem
//
// IMPORTANT: Commands should NOT call this function directly. Instead, use
// the centralized helper core.DiscoverAndSelectPacks which properly handles
// pack discovery, loading, and selection in a consistent way.
func GetPacksFS(candidates []string, filesystem types.FS) ([]types.Pack, error) {
	logger := logging.GetLogger("packs.discovery")
	logger.Trace().Int("count", len(candidates)).Msg("Validating pack candidates with FS")

	var packs []types.Pack

	for _, candidatePath := range candidates {
		pack, err := loadPackFS(candidatePath, filesystem)
		if err != nil {
			// Log the error but continue with other packs
			logger.Warn().
				Err(err).
				Str("path", candidatePath).
				Msg("Failed to load pack, skipping")
			continue
		}

		// Note: We include ignored packs in the list for status display
		// The actual processing will handle them appropriately

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

// loadPackFS creates a Pack instance from a directory path using the provided filesystem
func loadPackFS(packPath string, filesystem types.FS) (types.Pack, error) {
	logger := log.With().Str("path", packPath).Logger()

	// Verify the path exists and is accessible
	info, err := filesystem.Stat(packPath)
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

	// Validate pack name
	if err := paths.ValidatePackName(packName); err != nil {
		logger.Warn().Str("pack", packName).Err(err).Msg("Skipping pack with invalid name")
		return types.Pack{}, errors.Wrap(err, errors.ErrPackInvalid, "invalid pack name").
			WithDetail("pack", packName).
			WithDetail("path", packPath)
	}

	// Create base pack
	pack := types.Pack{
		Name:     packName,
		Path:     packPath,
		Metadata: make(map[string]interface{}),
	}

	// Load pack configuration if it exists
	configPath := filepath.Join(packPath, ".dodot.toml")
	if _, err := filesystem.Stat(configPath); err == nil {
		packConfig, err := loadPackConfigFS(configPath, filesystem)
		if err != nil {
			return types.Pack{}, errors.Wrap(err, errors.ErrConfigLoad, "failed to load pack config").
				WithDetail("pack", packName).
				WithDetail("configPath", configPath)
		}
		pack.Config = packConfig
	}

	logger.Trace().
		Str("name", pack.Name).
		Bool("hasConfig", err == nil).
		Msg("Pack loaded successfully")

	return pack, nil
}

// loadPackConfigFS reads and parses a pack's .dodot.toml configuration file using the provided filesystem
func loadPackConfigFS(configPath string, filesystem types.FS) (types.PackConfig, error) {
	data, err := filesystem.ReadFile(configPath)
	if err != nil {
		return types.PackConfig{}, err
	}

	// Parse the config from bytes
	var config types.PackConfig
	if err := toml.Unmarshal(data, &config); err != nil {
		return types.PackConfig{}, errors.Wrap(err, errors.ErrConfigParse, "failed to parse TOML")
	}

	return config, nil
}
