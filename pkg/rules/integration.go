package rules

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"

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

// GetPatternsForHandler returns all patterns that would activate a specific handler
// This includes patterns from both global and pack-specific rules
func GetPatternsForHandler(handlerName string, pack types.Pack) ([]string, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().
		Str("handler", handlerName).
		Str("pack", pack.Name).
		Msg("Getting patterns for handler")

	// Load global rules
	globalRules := config.GetRules()
	if len(globalRules) == 0 {
		globalRules = getDefaultRules()
	}

	// Load pack-specific rules
	packRules, err := LoadPackRules(pack.Path)
	if err != nil {
		logger.Debug().
			Err(err).
			Str("pack", pack.Name).
			Msg("No pack rules found, using global rules only")
	}

	// Merge rules (pack rules have higher priority)
	effectiveRules := MergeRules(globalRules, packRules)

	// Find all patterns that map to this handler
	patterns := []string{}
	seen := make(map[string]bool)

	for _, rule := range effectiveRules {
		// Skip exclusion patterns
		if strings.HasPrefix(rule.Pattern, "!") {
			continue
		}

		if rule.Handler == handlerName {
			// Avoid duplicates
			if !seen[rule.Pattern] {
				patterns = append(patterns, rule.Pattern)
				seen[rule.Pattern] = true
			}
		}
	}

	logger.Debug().
		Str("handler", handlerName).
		Int("patternCount", len(patterns)).
		Strs("patterns", patterns).
		Msg("Found patterns for handler")

	return patterns, nil
}

// GetAllHandlerPatterns returns patterns for all available handlers in a pack
// Returns a map of handler name to list of patterns
func GetAllHandlerPatterns(pack types.Pack) (map[string][]string, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().Str("pack", pack.Name).Msg("Getting all handler patterns")

	// Get all known handlers
	handlerNames := []string{"symlink", "shell", "homebrew", "install", "path"}

	result := make(map[string][]string)
	for _, handlerName := range handlerNames {
		patterns, err := GetPatternsForHandler(handlerName, pack)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal,
				"failed to get patterns for handler %s", handlerName)
		}
		if len(patterns) > 0 {
			result[handlerName] = patterns
		}
	}

	return result, nil
}

// SuggestFilenameForHandler suggests a filename that would match the handler's patterns
// For glob patterns, it returns a reasonable example filename
func SuggestFilenameForHandler(handlerName string, patterns []string) string {
	// If no patterns, return empty
	if len(patterns) == 0 {
		return ""
	}

	// Try to find the most specific pattern (non-glob)
	for _, pattern := range patterns {
		// Skip directory patterns
		if strings.HasSuffix(pattern, "/") {
			continue
		}

		// If it's not a glob pattern, use it directly
		if !strings.ContainsAny(pattern, "*?[]") {
			return pattern
		}
	}

	// Handle common glob patterns
	for _, pattern := range patterns {
		// Skip directory patterns
		if strings.HasSuffix(pattern, "/") {
			continue
		}

		switch {
		case strings.HasPrefix(pattern, "*") && strings.Count(pattern, "*") == 1:
			// Pattern like "*aliases.sh" -> "aliases.sh"
			return strings.TrimPrefix(pattern, "*")
		case strings.HasSuffix(pattern, "*") && strings.Count(pattern, "*") == 1:
			// Pattern like "profile*" -> "profile.sh"
			base := strings.TrimSuffix(pattern, "*")
			if handlerName == "shell" {
				return base + ".sh"
			}
			return base
		}
	}

	// Handler-specific defaults
	switch handlerName {
	case "shell":
		return "shell.sh"
	case "install":
		return "install.sh"
	case "homebrew":
		return "Brewfile"
	case "path":
		return "bin/"
	case "symlink":
		// For symlink (catchall), don't suggest a filename
		return ""
	default:
		return ""
	}
}

// GetHandlersNeedingFiles returns handlers that have no matching files in the pack
// It uses the existing matching system to determine which handlers are already active
// Optionally accepts a filesystem parameter for testing
func GetHandlersNeedingFiles(pack types.Pack, optionalFS ...types.FS) ([]string, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().Str("pack", pack.Name).Msg("Getting handlers needing files")

	// Use provided filesystem or nil (which will use default)
	var fs types.FS
	if len(optionalFS) > 0 {
		fs = optionalFS[0]
	}

	// Get all matches for this pack
	matches, err := GetMatchesFS([]types.Pack{pack}, fs)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal,
			"failed to get matches for pack %s", pack.Name)
	}

	// Track which handlers already have files
	activeHandlers := make(map[string]bool)
	for _, match := range matches {
		// Skip symlink handler as it's a catchall
		if match.HandlerName == "symlink" {
			continue
		}
		activeHandlers[match.HandlerName] = true
	}

	// Get all known handlers except symlink
	allHandlers := []string{"shell", "homebrew", "install", "path"}

	// Find handlers that need files
	var needingFiles []string
	for _, handler := range allHandlers {
		if !activeHandlers[handler] {
			needingFiles = append(needingFiles, handler)
		}
	}

	logger.Debug().
		Str("pack", pack.Name).
		Strs("handlers", needingFiles).
		Msg("Handlers needing files")

	return needingFiles, nil
}
