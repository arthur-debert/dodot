package core

import (
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetMatches processes packs and returns all files that match rules
func GetMatches(packs []types.Pack) ([]types.RuleMatch, error) {
	return GetMatchesFS(packs, filesystem.NewOS())
}

// GetMatchesFS processes packs and returns all files that match rules using the provided filesystem
func GetMatchesFS(packs []types.Pack, filesystem types.FS) ([]types.RuleMatch, error) {
	// For now, ignore the filesystem parameter and use the rules system
	// TODO: Update rules system to support custom filesystem
	return rules.GetMatches(packs)
}
