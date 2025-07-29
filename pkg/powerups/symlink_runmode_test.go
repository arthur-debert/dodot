package powerups

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestSymlinkPowerUp_RunMode(t *testing.T) {
	powerUp := NewSymlinkPowerUp()
	testutil.AssertEqual(t, types.RunModeMany, powerUp.RunMode())
}