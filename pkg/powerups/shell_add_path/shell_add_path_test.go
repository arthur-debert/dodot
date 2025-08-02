package shell_add_path

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestShellAddPathPowerUp(t *testing.T) {
	powerup := NewShellAddPathPowerUp()

	testutil.AssertEqual(t, ShellAddPathPowerUpName, powerup.Name())
	testutil.AssertEqual(t, types.RunModeMany, powerup.RunMode())

	matches := []types.TriggerMatch{
		{
			Path:         "bin",
			AbsolutePath: "/path/to/bin",
			Pack:         "bin-pack",
		},
	}

	actions, err := powerup.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(actions))

	action := actions[0]
	testutil.AssertEqual(t, types.ActionTypePathAdd, action.Type)
	testutil.AssertEqual(t, "/path/to/bin", action.Source)
	testutil.AssertEqual(t, "bin-pack", action.Pack)
}
