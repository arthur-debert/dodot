package shell_profile

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestShellProfilePowerUp(t *testing.T) {
	powerup := NewShellProfilePowerUp()

	testutil.AssertEqual(t, ShellProfilePowerUpName, powerup.Name())
	testutil.AssertEqual(t, types.RunModeMany, powerup.RunMode())

	matches := []types.TriggerMatch{
		{
			Path:         "aliases.sh",
			AbsolutePath: "/path/to/aliases.sh",
			Pack:         "shell-pack",
		},
	}

	actions, err := powerup.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(actions))

	action := actions[0]
	testutil.AssertEqual(t, types.ActionTypeShellSource, action.Type)
	testutil.AssertEqual(t, "/path/to/aliases.sh", action.Source)
	testutil.AssertEqual(t, "shell-pack", action.Pack)
}
