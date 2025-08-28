package matchers

import (
	"io/fs"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ScanPack applies matchers to files in a pack and returns matches.
// This function performs FLAT scanning of pack directories - only top-level
// files and directories within a pack are processed. Subdirectory contents
// are NOT recursively scanned or processed.
//
// The scanning process:
// 1. Reads immediate children of the pack directory
// 2. Filters out special files and ignored files
// 3. Handles pack config overrides
// 4. Applies matchers in priority order (specific then catchall)
// 5. Returns all matches found
func ScanPack(pack types.Pack, filesystem types.FS) ([]types.TriggerMatch, error) {
	// Use default matchers for backward compatibility
	return ScanPackWithMatchers(pack, filesystem, DefaultMatchers())
}

// ScanPackWithMatchers applies specific matchers to files in a pack and returns matches.
// This allows for pack-specific matcher configurations.
func ScanPackWithMatchers(pack types.Pack, filesystem types.FS, packMatchers []types.Matcher) ([]types.TriggerMatch, error) {
	logger := logging.GetLogger("matchers.scanner").With().
		Str("pack", pack.Name).
		Logger()

	logger.Debug().Msg("Scanning pack for matches")

	// Get config
	cfg := config.Default()

	// Check if we have matchers
	if len(packMatchers) == 0 {
		logger.Debug().Msg("No matchers configured for pack")
		return []types.TriggerMatch{}, nil
	}

	// Filter enabled matchers
	enabledMatchers := FilterEnabledMatchers(packMatchers)
	if len(enabledMatchers) == 0 {
		logger.Debug().Msg("No enabled matchers for pack")
		return []types.TriggerMatch{}, nil
	}

	// Sort matchers by priority
	SortMatchersByPriority(enabledMatchers)

	// Separate specific matchers from catchall
	var specificMatchers []types.Matcher
	var catchallMatchers []types.Matcher

	for _, matcher := range enabledMatchers {
		// Get trigger factory to check the type
		triggerFactory, err := registry.GetTriggerFactory(matcher.TriggerName)
		if err != nil {
			logger.Warn().
				Err(err).
				Str("matcher", matcher.Name).
				Msg("Failed to get trigger factory for matcher")
			continue
		}

		trigger, err := triggerFactory(matcher.TriggerOptions)
		if err != nil {
			logger.Warn().
				Err(err).
				Str("matcher", matcher.Name).
				Msg("Failed to create trigger for matcher")
			continue
		}

		if trigger.Type() == types.TriggerTypeCatchall {
			catchallMatchers = append(catchallMatchers, matcher)
		} else {
			specificMatchers = append(specificMatchers, matcher)
		}
	}

	var matches []types.TriggerMatch
	matchedFiles := make(map[string]bool)

	// FLAT SCAN: Read only the immediate children of the pack directory
	entries, err := filesystem.ReadDir(pack.Path)
	if err != nil {
		return nil, errors.Wrap(err, errors.ErrFileAccess, "failed to read pack directory")
	}

	// Process each top-level entry in the pack
	for _, entry := range entries {
		name := entry.Name()

		// Skip special files
		if name == cfg.Patterns.SpecialFiles.PackConfig {
			continue
		}

		path := filepath.Join(pack.Path, name)
		relPath := name // For top-level items, relative path is just the name

		// Check if file should be ignored by pack config
		if pack.Config.IsIgnored(relPath) {
			logger.Trace().Str("path", relPath).Msg("File ignored by pack config")
			continue
		}

		// Check for a behavior override from pack config
		if override := pack.Config.FindOverride(relPath); override != nil {
			logger.Trace().Str("path", relPath).Str("handler", override.Handler).Msg("File behavior overridden by pack config")
			match := types.TriggerMatch{
				TriggerName:    "config-override",
				Pack:           pack.Name,
				Path:           relPath,
				AbsolutePath:   path,
				Metadata:       make(map[string]interface{}),
				HandlerName:    override.Handler,
				HandlerOptions: override.With,
				Priority:       100, // Config overrides have high priority
			}
			matches = append(matches, match)
			matchedFiles[relPath] = true
			continue
		}

		// Get file info for default matching
		info, err := entry.Info()
		if err != nil {
			logger.Warn().Err(err).Str("path", path).Msg("Failed to get file info")
			continue
		}

		// Phase 1: Test against specific matchers first
		for _, matcher := range specificMatchers {
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
				matchedFiles[relPath] = true
				// Only one matcher can match per file
				break
			}
		}

		// Phase 2: If not matched by specific matchers, test against catchall matchers
		if !matchedFiles[relPath] && len(catchallMatchers) > 0 {
			for _, matcher := range catchallMatchers {
				match, err := testMatcher(pack, path, relPath, info, matcher)
				if err != nil {
					logger.Warn().
						Err(err).
						Str("matcher", matcher.Name).
						Str("path", path).
						Msg("Failed to test catchall matcher")
					continue
				}
				if match != nil {
					matches = append(matches, *match)
					matchedFiles[relPath] = true
					// Only one matcher can match per file
					break
				}
			}
		}
	}

	logger.Debug().
		Int("matchCount", len(matches)).
		Int("specificMatchers", len(specificMatchers)).
		Int("catchallMatchers", len(catchallMatchers)).
		Msg("Completed pack scanning")

	return matches, nil
}

// testMatcher tests if a file matches a matcher's trigger
func testMatcher(pack types.Pack, absPath, relPath string, info fs.FileInfo, matcher types.Matcher) (*types.TriggerMatch, error) {
	logger := logging.GetLogger("matchers.scanner")

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

	logger.Trace().
		Str("trigger", matcher.TriggerName).
		Str("handler", matcher.HandlerName).
		Str("path", relPath).
		Msg("Trigger matched")

	// Create trigger match
	match := &types.TriggerMatch{
		TriggerName:    matcher.TriggerName,
		Pack:           pack.Name,
		Path:           relPath,
		AbsolutePath:   absPath,
		Metadata:       metadata,
		HandlerName:    matcher.HandlerName,
		HandlerOptions: matcher.HandlerOptions,
		Priority:       matcher.Priority,
	}

	// Initialize handler options from matcher
	if match.HandlerOptions == nil {
		match.HandlerOptions = make(map[string]interface{})
	}

	// Copy matcher-level handler options
	for k, v := range matcher.HandlerOptions {
		match.HandlerOptions[k] = v
	}

	return match, nil
}
