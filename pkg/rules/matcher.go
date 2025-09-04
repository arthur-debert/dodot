package rules

import (
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// Matcher handles the matching of files against rules for packs
type Matcher struct {
	logger zerolog.Logger
}

// NewMatcher creates a new rule matcher
func NewMatcher() *Matcher {
	return &Matcher{
		logger: logging.GetLogger("rules.matcher"),
	}
}

// GetMatches scans packs and returns all rule matches using the rule system
func (m *Matcher) GetMatches(packs []types.Pack) ([]RuleMatch, error) {
	m.logger.Debug().Int("packCount", len(packs)).Msg("Getting matches for packs")

	// Load global rules
	globalRules := config.GetRules()
	if len(globalRules) == 0 {
		m.logger.Debug().Msg("No rules from config, using defaults")
		globalRules = GetDefaultRules()
	}
	m.logger.Debug().
		Int("ruleCount", len(globalRules)).
		Msg("Loaded global rules")

	scanner := NewScanner(globalRules)
	var allRuleMatches []RuleMatch

	// Process each pack
	for _, pack := range packs {
		m.logger.Debug().Str("pack", pack.Name).Msg("Processing pack")

		// Load pack-specific rules
		packRules, err := LoadPackRules(pack.Path)
		if err != nil {
			m.logger.Warn().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to load pack rules, using global rules only")
		}

		// Merge rules (pack rules have higher priority)
		effectiveRules := MergeRules(globalRules, packRules)
		scanner.rules = effectiveRules

		// Scan the pack
		matches, err := scanner.ScanPack(pack)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal,
				"failed to scan pack %s", pack.Name)
		}

		// Convert rule matches to RuleMatch type
		for _, match := range matches {
			ruleMatch := RuleMatch{
				RuleName:     "rule-based",
				Pack:         pack.Name,
				Path:         match.FilePath,
				AbsolutePath: filepath.Join(pack.Path, match.FilePath),
				Metadata: map[string]interface{}{
					"filename":     match.FileName,
					"is_directory": match.IsDirectory,
					"pattern":      "rule-based", // Indicate this came from rules
				},
				HandlerName:    match.Handler,
				HandlerOptions: match.Options,
				Priority:       0, // Priority is handled by rule order
			}
			allRuleMatches = append(allRuleMatches, ruleMatch)
		}
	}

	m.logger.Info().
		Int("totalMatches", len(allRuleMatches)).
		Msg("Completed matching across all packs")

	return allRuleMatches, nil
}

// GetMatchesFS scans packs and returns all rule matches using the rule system with a custom filesystem
// This function is used for testing and commands that need to use a different filesystem
func (m *Matcher) GetMatchesFS(packs []types.Pack, fs types.FS) ([]RuleMatch, error) {
	m.logger.Debug().Int("packCount", len(packs)).Msg("Getting matches for packs with FS")

	// Load global rules
	globalRules := config.GetRules()
	if len(globalRules) == 0 {
		m.logger.Debug().Msg("No rules from config, using defaults")
		globalRules = GetDefaultRules()
	}

	scanner := NewScannerWithFS(globalRules, fs)
	var allRuleMatches []RuleMatch

	// Process each pack
	for _, pack := range packs {
		m.logger.Debug().Str("pack", pack.Name).Msg("Processing pack with FS")

		// Load pack-specific rules
		packRules, err := LoadPackRulesFS(pack.Path, fs)
		if err != nil {
			m.logger.Warn().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to load pack rules, using global rules only")
		}

		// Merge rules (pack rules have higher priority)
		effectiveRules := MergeRules(globalRules, packRules)
		scanner.rules = effectiveRules

		// Scan the pack
		matches, err := scanner.ScanPack(pack)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal,
				"failed to scan pack %s", pack.Name)
		}

		// Convert rule matches to RuleMatch type
		for _, match := range matches {
			ruleMatch := RuleMatch{
				RuleName:     "rule-based",
				Pack:         pack.Name,
				Path:         match.FilePath,
				AbsolutePath: filepath.Join(pack.Path, match.FilePath),
				Metadata: map[string]interface{}{
					"filename":     match.FileName,
					"is_directory": match.IsDirectory,
					"pattern":      "rule-based", // Indicate this came from rules
				},
				HandlerName:    match.Handler,
				HandlerOptions: match.Options,
				Priority:       0, // Priority is handled by rule order
			}
			allRuleMatches = append(allRuleMatches, ruleMatch)
		}
	}

	m.logger.Info().
		Int("totalMatches", len(allRuleMatches)).
		Msg("Completed matching across all packs with FS")

	return allRuleMatches, nil
}
