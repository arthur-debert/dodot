package core

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// TestProcessMatch tests the deprecated ProcessMatch function for backward compatibility
func TestProcessMatch(t *testing.T) {
	// Single match to process
	match := types.TriggerMatch{
		TriggerName:  "filename",
		Pack:         "test-pack",
		Path:         ".vimrc",
		AbsolutePath: "/test/pack/.vimrc",
		PowerUpName:  "symlink",
		PowerUpOptions: map[string]interface{}{
			"target": "~",
		},
		Priority: 1,
		Metadata: map[string]interface{}{},
	}

	// Process the single match
	actions, err := ProcessMatch(match)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(actions))
	
	// Verify it produces the same result as ProcessMatchGroup
	groupActions, err := ProcessMatchGroup([]types.TriggerMatch{match})
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, len(actions), len(groupActions))
	
	if len(actions) > 0 && len(groupActions) > 0 {
		testutil.AssertEqual(t, actions[0].Type, groupActions[0].Type)
		testutil.AssertEqual(t, actions[0].Source, groupActions[0].Source)
		testutil.AssertEqual(t, actions[0].Target, groupActions[0].Target)
	}
}