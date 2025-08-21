// Package core provides the core processing pipeline for dodot.
//
// IMPORTANT DESIGN PRINCIPLE: Pack Scanning is FLAT
// =================================================
// Packs are scanned as flat directories - only top-level entries are processed.
// This is a fundamental design decision in dodot:
//
// 1. When scanning a pack, we read ONLY the immediate children
// 2. We do NOT recursively traverse subdirectories
// 3. Directories are processed as single units (e.g., for symlinking the entire dir)
// 4. Files inside subdirectories are NOT individually scanned or matched
//
// This design allows handlers to handle entire directory trees (like symlinking
// a whole config directory) without dodot trying to process individual files
// within those directories.
//
// Example:
//
//	pack/
//	├── file.txt        ✓ Processed
//	├── dir/            ✓ Processed as a unit
//	│   └── nested.txt  ✗ NOT processed (part of dir/)
//	└── another.sh      ✓ Processed
package core

import (
	"io/fs"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/matchers"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetFiringTriggers processes packs and returns all triggers that match files
// Deprecated: Use GetFiringTriggersFS instead to support filesystem abstraction
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

// GetFiringTriggersFS processes packs and returns all triggers that match files using the provided filesystem
func GetFiringTriggersFS(packs []types.Pack, filesystem types.FS) ([]types.TriggerMatch, error) {
	logger := logging.GetLogger("core.triggers")
	logger.Debug().Int("packCount", len(packs)).Msg("Getting firing triggers with FS")

	var allMatches []types.TriggerMatch

	// Process each pack
	for _, pack := range packs {
		matches, err := ProcessPackTriggersFS(pack, filesystem)
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
// IMPORTANT: This function performs FLAT scanning of pack directories.
// Only top-level files and directories within a pack are processed.
// Subdirectory contents are NOT recursively scanned or processed.
// This is a core design principle of dodot - packs are treated as flat directories.
//
// Example:
//
//	nvim/                    # pack
//	├── install.sh          # ✓ processed - triggers install handler
//	├── bin/                # ✓ processed - triggers path handler
//	│   └── alias.sh        # ✗ NOT processed - inside subdirectory
//	└── lua/                # ✓ processed - triggers symlink handler
//	    └── install.sh      # ✗ NOT processed - inside subdirectory
//
// Deprecated: Use ProcessPackTriggersFS instead to support filesystem abstraction
func ProcessPackTriggers(pack types.Pack) ([]types.TriggerMatch, error) {
	logger := logging.GetLogger("core.triggers").With().
		Str("pack", pack.Name).
		Logger()

	logger.Debug().Msg("Processing pack triggers (flat scan)")

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

	// Separate matchers by trigger type
	var specificMatchers []types.Matcher
	var catchallMatchers []types.Matcher

	for _, matcher := range enabledMatchers {
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
	// This is intentionally NOT recursive - subdirectories are not traversed
	entries, err := fs.ReadDir(os.DirFS(pack.Path), ".")
	if err != nil {
		return nil, errors.Wrap(err, errors.ErrFileAccess, "failed to read pack directory")
	}

	// Process each top-level entry in the pack
	for _, entry := range entries {
		name := entry.Name()

		// Skip .dodot.toml files
		if name == ".dodot.toml" {
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
				TriggerName:    "override-rule",
				Pack:           pack.Name,
				Path:           relPath,
				AbsolutePath:   path,
				Metadata:       make(map[string]interface{}),
				HandlerName:    override.Handler,
				HandlerOptions: override.With,
				Priority:       types.OverridePriority, // High priority for overrides
			}
			matches = append(matches, match)
			matchedFiles[relPath] = true
			continue // Don't process default matchers
		}

		// Get file info for default matching
		info, err := entry.Info()
		if err != nil {
			logger.Warn().Err(err).Str("path", path).Msg("Failed to get file info")
			continue
		}

		// Phase 1: Test against specific matchers
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
		Msg("Completed pack trigger processing (flat scan)")

	return matches, nil
}

// ProcessPackTriggersFS processes triggers for a single pack using the provided filesystem
// IMPORTANT: This function performs FLAT scanning of pack directories.
// Only top-level files and directories within a pack are processed.
// Subdirectory contents are NOT recursively scanned or processed.
// This is a core design principle of dodot - packs are treated as flat directories.
func ProcessPackTriggersFS(pack types.Pack, filesystem types.FS) ([]types.TriggerMatch, error) {
	logger := logging.GetLogger("core.triggers").With().
		Str("pack", pack.Name).
		Logger()

	logger.Debug().Msg("Processing pack triggers with FS (flat scan)")

	// Get matchers from pack config, merging with defaults
	packMatchers := getPackMatchers(pack)
	if len(packMatchers) == 0 {
		logger.Debug().Msg("No matchers configured for pack")
		return []types.TriggerMatch{}, nil
	}

	// Filter enabled matchers
	enabledMatchers := matchers.FilterEnabledMatchers(packMatchers)
	if len(enabledMatchers) == 0 {
		logger.Debug().Msg("No enabled matchers for pack")
		return []types.TriggerMatch{}, nil
	}

	// Sort matchers by priority
	matchers.SortMatchersByPriority(enabledMatchers)

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
	// This is intentionally NOT recursive - subdirectories are not traversed
	entries, err := filesystem.ReadDir(pack.Path)
	if err != nil {
		return nil, errors.Wrap(err, errors.ErrFileAccess, "failed to read pack directory")
	}

	// Process each top-level entry in the pack
	for _, entry := range entries {
		name := entry.Name()

		// Skip .dodot.toml files
		if name == ".dodot.toml" {
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
		Msg("Completed pack trigger processing (flat scan)")

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

	// Initialize power-up options from matcher
	if match.HandlerOptions == nil {
		match.HandlerOptions = make(map[string]interface{})
	}

	// Copy matcher-level power-up options
	for k, v := range matcher.HandlerOptions {
		match.HandlerOptions[k] = v
	}

	return match, nil
}
