package core

import (
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handler/pipeline"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetMatches processes packs and returns all files that match rules
func GetMatches(packs []types.Pack) ([]pipeline.RuleMatch, error) {
	return GetMatchesFS(packs, filesystem.NewOS())
}

// GetMatchesFS processes packs and returns all files that match rules using the provided filesystem
func GetMatchesFS(packs []types.Pack, filesystem types.FS) ([]pipeline.RuleMatch, error) {
	return pipeline.GetMatchesFS(packs, filesystem)
}

// FilterMatchesByHandlerCategory filters rule matches based on handler category
func FilterMatchesByHandlerCategory(matches []pipeline.RuleMatch, allowConfiguration, allowCodeExecution bool) []pipeline.RuleMatch {
	var filtered []pipeline.RuleMatch

	for _, match := range matches {
		// Check if handler is configuration type
		if allowConfiguration && handlers.HandlerRegistry.IsConfigurationHandler(match.HandlerName) {
			filtered = append(filtered, match)
		}
		// Check if handler is code execution type
		if allowCodeExecution && handlers.HandlerRegistry.IsCodeExecutionHandler(match.HandlerName) {
			filtered = append(filtered, match)
		}
	}

	return filtered
}
