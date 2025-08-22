package shell_add_path

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestShellAddPathHandler(t *testing.T) {
	handler := NewShellAddPathHandler()

	testutil.AssertEqual(t, ShellAddPathHandlerName, handler.Name())
	testutil.AssertEqual(t, types.RunModeLinking, handler.RunMode())

	matches := []types.TriggerMatch{
		{
			Path:         "bin",
			AbsolutePath: "/path/to/bin",
			Pack:         "bin-pack",
		},
	}

	actions, err := handler.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(actions))

	action := actions[0]
	testutil.AssertEqual(t, types.ActionTypePathAdd, action.Type)
	testutil.AssertEqual(t, "/path/to/bin", action.Source)
	testutil.AssertEqual(t, "bin-pack", action.Pack)
}
