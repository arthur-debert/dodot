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

func TestRuleMatch_Structure(t *testing.T) {
	match := types.RuleMatch{
		RuleName:     "filename",
		Pack:         "test-pack",
		Path:         "file.txt",
		AbsolutePath: "/test/file.txt",
		Priority:     10,
		Metadata: map[string]interface{}{
			"pattern": "*.txt",
		},
		HandlerName:    "symlink",
		HandlerOptions: map[string]interface{}{},
	}

	assert.Equal(t, "test-pack", match.Pack)
	assert.Equal(t, "file.txt", match.Path)
	assert.Equal(t, "/test/file.txt", match.AbsolutePath)
	assert.Equal(t, 10, match.Priority)
	assert.Equal(t, "filename", match.RuleName)
	assert.Equal(t, "symlink", match.HandlerName)

	// Check metadata
	assert.Contains(t, match.Metadata, "pattern")
	assert.Equal(t, "*.txt", match.Metadata["pattern"])
}
