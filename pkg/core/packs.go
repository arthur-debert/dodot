package core

import (
	"github.com/arthur-debert/dodot/pkg/packs"
)

// Re-exports from pkg/packs for backwards compatibility
var (
	GetPackCandidates = packs.GetPackCandidates
	GetPacks          = packs.GetPacks
	ValidatePack      = packs.ValidatePack
	shouldIgnorePack  = packs.ShouldIgnorePack
	SelectPacks       = packs.SelectPacks
	getPackNames      = packs.GetPackNames
)
