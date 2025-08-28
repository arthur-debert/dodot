package core

import (
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetMatchesSimplified uses the new rule-based system to find matches
// This will eventually replace the existing GetMatches function
func GetMatchesSimplified(packs []types.Pack) ([]types.TriggerMatch, error) {
	return rules.GetMatches(packs)
}
