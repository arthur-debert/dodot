package rules

import (
	"fmt"
	"path/filepath"
	"sort"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/install"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/shell"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetMatches scans packs and returns all rule matches using the rule system
func GetMatches(packs []types.Pack) ([]types.RuleMatch, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().Int("packCount", len(packs)).Msg("Getting matches for packs")

	// Load global rules
	globalRules := config.GetRules()
	if len(globalRules) == 0 {
		logger.Debug().Msg("No rules from config, using defaults")
		globalRules = getDefaultRules()
	}
	logger.Debug().
		Int("ruleCount", len(globalRules)).
		Msg("Loaded global rules")

	scanner := NewScanner(globalRules)
	var allRuleMatches []types.RuleMatch

	// Process each pack
	for _, pack := range packs {
		logger.Debug().Str("pack", pack.Name).Msg("Processing pack")

		// Load pack-specific rules
		packRules, err := LoadPackRules(pack.Path)
		if err != nil {
			logger.Warn().
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
			ruleMatch := types.RuleMatch{
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

	logger.Info().
		Int("totalMatches", len(allRuleMatches)).
		Msg("Completed matching across all packs")

	return allRuleMatches, nil
}

// GetMatchesFS scans packs and returns all rule matches using the rule system with a custom filesystem
// This function is used for testing and commands that need to use a different filesystem
func GetMatchesFS(packs []types.Pack, fs types.FS) ([]types.RuleMatch, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().Int("packCount", len(packs)).Msg("Getting matches for packs with FS")

	// Load global rules
	globalRules := config.GetRules()
	if len(globalRules) == 0 {
		globalRules = getDefaultRules()
	}

	scanner := NewScannerWithFS(globalRules, fs)
	var allRuleMatches []types.RuleMatch

	// Process each pack
	for _, pack := range packs {
		logger.Debug().Str("pack", pack.Name).Msg("Processing pack")

		// Load pack-specific rules
		packRules, err := LoadPackRulesFS(pack.Path, fs)
		if err != nil {
			logger.Warn().
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
			ruleMatch := types.RuleMatch{
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

	logger.Info().
		Int("totalMatches", len(allRuleMatches)).
		Msg("Completed matching across all packs")

	return allRuleMatches, nil
}

// CreateHandler creates a handler instance by name
// This replaces the registry-based handler creation
func CreateHandler(name string, options map[string]interface{}) (interface{}, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().
		Str("handler", name).
		Interface("options", options).
		Msg("Creating handler")

	switch name {
	case "symlink":
		h := symlink.NewSymlinkHandler()
		return h, nil
	case "shell":
		h := shell.NewShellHandler()
		return h, nil
	case "homebrew":
		h := homebrew.NewHomebrewHandler()
		return h, nil
	case "install":
		h := install.NewInstallHandler()
		return h, nil
	case "path":
		h := path.NewPathHandler()
		return h, nil
	default:
		return nil, fmt.Errorf("unknown handler: %s", name)
	}
}

// GroupMatchesByHandler groups rule matches by their handler name
func GroupMatchesByHandler(matches []types.RuleMatch) map[string][]types.RuleMatch {
	grouped := make(map[string][]types.RuleMatch)
	for _, match := range matches {
		handler := match.HandlerName
		grouped[handler] = append(grouped[handler], match)
	}
	return grouped
}

// GetHandlerExecutionOrder returns handlers in the order they should be executed
// Code execution handlers run before configuration handlers
func GetHandlerExecutionOrder(handlerNames []string) []string {
	type handlerInfo struct {
		name     string
		category handlers.HandlerCategory
	}

	var handlerList []handlerInfo
	for _, name := range handlerNames {
		// Validate handler exists
		_, err := CreateHandler(name, nil)
		if err != nil {
			continue
		}

		handlerList = append(handlerList, handlerInfo{
			name:     name,
			category: handlers.HandlerRegistry.GetHandlerCategory(name),
		})
	}

	// Sort: code execution handlers first, then configuration handlers
	sort.Slice(handlerList, func(i, j int) bool {
		if handlerList[i].category == handlerList[j].category {
			return handlerList[i].name < handlerList[j].name // alphabetical within same category
		}
		// Code execution comes before configuration
		return handlerList[i].category == handlers.CategoryCodeExecution
	})

	// Extract sorted names
	sorted := make([]string, len(handlerList))
	for i, h := range handlerList {
		sorted[i] = h.name
	}

	return sorted
}
