package symlink

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestSymlinkHandler_RunMode(t *testing.T) {
	handler := NewSymlinkHandler()
	testutil.AssertEqual(t, types.RunModeLinking, handler.RunMode())
}
