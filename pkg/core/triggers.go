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

		// Check file rules from pack config first
		fileAction := pack.Config.GetFileAction(relPath)
		if fileAction == "ignore" {
			logger.Debug().
				Str("path", relPath).
				Msg("File ignored by pack config")
			return nil
		}

		// If file action specifies a power-up, use it directly
		if fileAction != "" {
			match := types.TriggerMatch{
				TriggerName:    "file-rule",
				Pack:           pack.Name,
				Path:           relPath,
				AbsolutePath:   path,
				Metadata:       make(map[string]interface{}),
				PowerUpName:    fileAction,
				PowerUpOptions: make(map[string]interface{}),
				Priority:       0,
			}
			matches = append(matches, match)
			return nil
		}

		// Otherwise, test against default matchers
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

// getPackMatchers returns the default matchers for a pack
func getPackMatchers(pack types.Pack) []types.Matcher {
	// With simplified config, we only use default matchers
	// File-specific rules are handled separately
	return matchers.DefaultMatchers()
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

	// Initialize power-up options from matcher
	if match.PowerUpOptions == nil {
		match.PowerUpOptions = make(map[string]interface{})
	}

	// Copy matcher-level power-up options
	for k, v := range matcher.PowerUpOptions {
		match.PowerUpOptions[k] = v
	}

	return match, nil
}
