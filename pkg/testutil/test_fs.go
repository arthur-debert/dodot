package testutil

import (
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/spf13/afero"
)

// NewTestFS creates a new in-memory filesystem for testing.
func NewTestFS() types.FS {
	return filesystem.NewAferoFS(afero.NewMemMapFs())
}
