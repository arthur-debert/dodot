package core

import (
	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/matchers"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetMatches processes packs and returns all triggers that match files
// Deprecated: Use GetMatchesFS instead to support filesystem abstraction
func GetMatches(packs []types.Pack) ([]types.TriggerMatch, error) {
	return GetMatchesFS(packs, filesystem.NewOS())
}

// GetMatchesFS processes packs and returns all triggers that match files using the provided filesystem
func GetMatchesFS(packs []types.Pack, filesystem types.FS) ([]types.TriggerMatch, error) {
	logger := logging.GetLogger("core.match")
	logger.Debug().Int("packCount", len(packs)).Msg("Getting matches")

	// Load global config once
	globalConfig, err := config.LoadConfiguration()
	if err != nil {
		logger.Error().Err(err).Msg("Failed to load global configuration")
		return nil, err
	}

	var allMatches []types.TriggerMatch

	// Process each pack with its own config
	for _, pack := range packs {
		// Load pack-specific config if it exists
		packConfig, err := config.LoadPackConfiguration(globalConfig, pack.Path)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to load pack configuration")
			return nil, err
		}

		// Get matchers for this pack (includes defaults + file_mapping)
		packMatchers := matchers.ConvertConfigMatchers(packConfig.Matchers)

		// Scan pack with its specific matchers
		matches, err := matchers.ScanPackWithMatchers(pack, filesystem, packMatchers)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to scan pack for matches")
			return nil, err
		}
		allMatches = append(allMatches, matches...)
	}

	logger.Info().Int("matchCount", len(allMatches)).Msg("Found trigger matches")
	return allMatches, nil
}
