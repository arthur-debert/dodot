package pack

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// Pack wraps types.Pack to add higher-level operations that require
// dependencies on other packages (avoiding circular imports).
type Pack struct {
	*types.Pack // Embed the types.Pack
}

// New creates a new Pack wrapper from a types.Pack
func New(p *types.Pack) *Pack {
	return &Pack{Pack: p}
}

// AsTypesPack returns the underlying types.Pack
// This is useful when you need to pass it to functions expecting *types.Pack
func (p *Pack) AsTypesPack() *types.Pack {
	return p.Pack
}
