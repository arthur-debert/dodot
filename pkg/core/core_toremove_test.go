package core

import (
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/types"
)

// testPaths implements types.Pather for testing
type testPaths struct {
	dataDir string
}

func (p *testPaths) DotfilesRoot() string {
	return "dotfiles"
}

func (p *testPaths) DataDir() string {
	return p.dataDir
}

func (p *testPaths) ConfigDir() string {
	return filepath.Join(p.dataDir, "config")
}

func (p *testPaths) CacheDir() string {
	return filepath.Join(p.dataDir, "cache")
}

func (p *testPaths) StateDir() string {
	return filepath.Join(p.dataDir, "state")
}

// NewTestPaths creates a new testPaths instance for testing
func NewTestPaths(dataDir string) types.Pather {
	return &testPaths{dataDir: dataDir}
}
