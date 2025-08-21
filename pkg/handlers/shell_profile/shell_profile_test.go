package shell_profile

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestShellProfileHandler(t *testing.T) {
	handler := NewShellProfileHandler()

	testutil.AssertEqual(t, ShellProfileHandlerName, handler.Name())
	testutil.AssertEqual(t, types.RunModeMany, handler.RunMode())

	matches := []types.TriggerMatch{
		{
			Path:         "aliases.sh",
			AbsolutePath: "/path/to/aliases.sh",
			Pack:         "shell-pack",
		},
	}

	actions, err := handler.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(actions))

	action := actions[0]
	testutil.AssertEqual(t, types.ActionTypeShellSource, action.Type)
	testutil.AssertEqual(t, "/path/to/aliases.sh", action.Source)
	testutil.AssertEqual(t, "shell-pack", action.Pack)
}
