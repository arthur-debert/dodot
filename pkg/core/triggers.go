package core

import (
	"io/fs"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/matchers"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetFiringTriggers processes packs and returns all triggers that match files
func GetFiringTriggers(packs []types.Pack) ([]types.TriggerMatch, error) {
	logger := logging.GetLogger("core.triggers")
	logger.Debug().Int("packCount", len(packs)).Msg("Getting firing triggers")

	var allMatches []types.TriggerMatch

	// Process each pack
	for _, pack := range packs {
		matches, err := ProcessPackTriggers(pack)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to process pack triggers")
			return nil, err
		}
		allMatches = append(allMatches, matches...)
	}
	
	logger.Info().Int("matchCount", len(allMatches)).Msg("Found trigger matches")
	return allMatches, nil
}

// ProcessPackTriggers processes triggers for a single pack
func ProcessPackTriggers(pack types.Pack) ([]types.TriggerMatch, error) {
	logger := logging.GetLogger("core.triggers").With().
		Str("pack", pack.Name).
		Logger()
	
	logger.Debug().Msg("Processing pack triggers")

	// Get matchers from pack config, merging with defaults
	packMatchers := getPackMatchers(pack)
	if len(packMatchers) == 0 {
		logger.Debug().Msg("No matchers configured for pack")
		return nil, nil
	}

	// Filter enabled matchers
	enabledMatchers := matchers.FilterEnabledMatchers(packMatchers)
	if len(enabledMatchers) == 0 {
		logger.Debug().Msg("No enabled matchers for pack")
		return nil, nil
	}

	// Sort matchers by priority
	matchers.SortMatchersByPriority(enabledMatchers)

	var matches []types.TriggerMatch

	// Walk the pack directory
	err := filepath.WalkDir(pack.Path, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}

		// Skip the pack root itself
		if path == pack.Path {
			return nil
		}

		// Skip .dodot.toml files
		if filepath.Base(path) == ".dodot.toml" {
			return nil
		}

		// Skip ignored patterns
		if shouldIgnore(filepath.Base(path)) {
			if d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}

		// Get file info
		info, err := d.Info()
		if err != nil {
			logger.Warn().
				Err(err).
				Str("path", path).
				Msg("Failed to get file info")
			return nil
		}

		// Get relative path within pack
		relPath, err := filepath.Rel(pack.Path, path)
		if err != nil {
			logger.Warn().
				Err(err).
				Str("path", path).
				Msg("Failed to get relative path")
			return nil
		}

		// Test against each matcher
		for _, matcher := range enabledMatchers {
			match, err := testMatcher(pack, path, relPath, info, matcher)
			if err != nil {
				logger.Warn().
					Err(err).
					Str("matcher", matcher.Name).
					Str("path", path).
					Msg("Failed to test matcher")
				continue
			}
			if match != nil {
				matches = append(matches, *match)
				// Only one matcher can match per file
				break
			}
		}

		return nil
	})

	if err != nil {
		return nil, errors.Wrap(err, errors.ErrFileAccess, "failed to walk pack directory")
	}

	logger.Debug().
		Int("matchCount", len(matches)).
		Msg("Completed pack trigger processing")

	return matches, nil
}

// getPackMatchers returns the matchers for a pack, merging defaults with pack-specific ones
func getPackMatchers(pack types.Pack) []types.Matcher {
	logger := logging.GetLogger("core.triggers")

	// Start with default matchers
	defaultMatchers := matchers.DefaultMatchers()

	// Convert pack's MatcherConfig to Matcher
	var packMatchers []types.Matcher
	for _, config := range pack.Config.Matchers {
		matcher, err := matchers.CreateMatcher(&config)
		if err != nil {
			logger.Warn().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to create matcher from config")
			continue
		}
		packMatchers = append(packMatchers, *matcher)
	}

	// Merge matchers (pack-specific override defaults)
	return matchers.MergeMatchers(defaultMatchers, packMatchers)
}

// testMatcher tests if a file matches a matcher's trigger
func testMatcher(pack types.Pack, absPath, relPath string, info fs.FileInfo, matcher types.Matcher) (*types.TriggerMatch, error) {
	logger := logging.GetLogger("core.triggers")

	// Get trigger factory
	triggerFactory, err := registry.GetTriggerFactory(matcher.TriggerName)
	if err != nil {
		return nil, err
	}

	// Create trigger instance with options
	trigger, err := triggerFactory(matcher.TriggerOptions)
	if err != nil {
		return nil, err
	}

	// Test if file matches
	matched, metadata := trigger.Match(relPath, info)
	if !matched {
		return nil, nil
	}

	logger.Debug().
		Str("trigger", matcher.TriggerName).
		Str("powerup", matcher.PowerUpName).
		Str("path", relPath).
		Msg("Trigger matched")

	// Create trigger match
	match := &types.TriggerMatch{
		TriggerName:    matcher.TriggerName,
		Pack:           pack.Name,
		Path:           relPath,
		AbsolutePath:   absPath,
		Metadata:       metadata,
		PowerUpName:    matcher.PowerUpName,
		PowerUpOptions: matcher.PowerUpOptions,
		Priority:       matcher.Priority,
	}

	// Merge power-up options from different sources
	if match.PowerUpOptions == nil {
		match.PowerUpOptions = make(map[string]interface{})
	}

	// Pack-level power-up options
	if packOpts, ok := pack.Config.PowerUpOptions[matcher.PowerUpName]; ok {
		for k, v := range packOpts {
			match.PowerUpOptions[k] = v
		}
	}

	// Matcher-level power-up options (override pack-level)
	for k, v := range matcher.PowerUpOptions {
		match.PowerUpOptions[k] = v
	}

	return match, nil
}