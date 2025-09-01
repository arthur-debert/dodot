// pkg/types/types_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: None
// PURPOSE: Test basic type structures

package types_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestPack_Structure(t *testing.T) {
	pack := types.Pack{
		Name: "test-pack",
		Path: "/path/to/pack",
		Config: config.PackConfig{
			Ignore: []config.IgnoreRule{
				{Path: "*.bak"},
			},
		},
	}

	assert.Equal(t, "test-pack", pack.Name)
	assert.Equal(t, "/path/to/pack", pack.Path)
	assert.Len(t, pack.Config.Ignore, 1)
	assert.Equal(t, "*.bak", pack.Config.Ignore[0].Path)
}
