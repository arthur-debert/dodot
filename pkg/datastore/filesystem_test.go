package datastore_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/datastore/testutil"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/suite"
)

type DataStoreSuite struct {
	suite.Suite
	fs      *testutil.MockFS
	paths   paths.Paths
	tempDir string
}

func (s *DataStoreSuite) SetupTest() {
	s.tempDir = s.T().TempDir()
	s.fs = testutil.NewMockFS()

	// Create a paths instance pointing to our temp dir
	s.T().Setenv("DOTFILES_ROOT", filepath.Join(s.tempDir, "dotfiles"))
	s.T().Setenv("DODOT_DATA_DIR", filepath.Join(s.tempDir, "data"))

	var err error
	s.paths, err = paths.New("")
	s.Require().NoError(err)

	// Create necessary base directories
	s.Require().NoError(s.fs.MkdirAll(s.paths.DotfilesRoot(), 0755))
	s.Require().NoError(s.fs.MkdirAll(s.paths.DataDir(), 0755))
}

func TestDataStoreSuite(t *testing.T) {
	suite.Run(t, new(DataStoreSuite))
}

func (s *DataStoreSuite) TestLink_NewLink() {
	ds := datastore.New(s.fs, s.paths)

	packName := "vim"
	sourceFileDir := filepath.Join(s.paths.DotfilesRoot(), packName)
	sourceFilePath := filepath.Join(sourceFileDir, ".vimrc")
	s.Require().NoError(s.fs.MkdirAll(sourceFileDir, 0755))
	s.Require().NoError(s.fs.WriteFile(sourceFilePath, []byte("vim config"), 0644))

	intermediatePath, err := ds.Link(packName, sourceFilePath)
	s.Require().NoError(err)

	expectedIntermediatePath := filepath.Join(s.paths.DataDir(), "packs", packName, "symlinks", ".vimrc")
	s.Equal(expectedIntermediatePath, intermediatePath)

	// Verify the symlink was created and points to the correct source
	target, err := s.fs.Readlink(intermediatePath)
	s.Require().NoError(err)
	s.Equal(sourceFilePath, target)
}

func (s *DataStoreSuite) TestLink_ExistingCorrectLink() {
	ds := datastore.New(s.fs, s.paths)
	packName := "vim"
	sourceFileDir := filepath.Join(s.paths.DotfilesRoot(), packName)
	sourceFilePath := filepath.Join(sourceFileDir, ".vimrc")
	s.Require().NoError(s.fs.MkdirAll(sourceFileDir, 0755))
	s.Require().NoError(s.fs.WriteFile(sourceFilePath, []byte("vim config"), 0644))

	// Create the link once
	_, err := ds.Link(packName, sourceFilePath)
	s.Require().NoError(err)

	// Call Link again
	intermediatePath, err := ds.Link(packName, sourceFilePath)
	s.Require().NoError(err)

	// Should return the same path and no error
	expectedIntermediatePath := filepath.Join(s.paths.DataDir(), "packs", packName, "symlinks", ".vimrc")
	s.Equal(expectedIntermediatePath, intermediatePath)
}

func (s *DataStoreSuite) TestLink_ExistingIncorrectLink() {
	ds := datastore.New(s.fs, s.paths)
	packName := "vim"
	sourceFileDir := filepath.Join(s.paths.DotfilesRoot(), packName)
	sourceFilePath := filepath.Join(sourceFileDir, ".vimrc")
	incorrectSourcePath := filepath.Join(sourceFileDir, "old.vimrc")
	s.Require().NoError(s.fs.MkdirAll(sourceFileDir, 0755))
	s.Require().NoError(s.fs.WriteFile(sourceFilePath, []byte("vim config"), 0644))
	s.Require().NoError(s.fs.WriteFile(incorrectSourcePath, []byte("old vim config"), 0644))

	intermediateDir := filepath.Join(s.paths.DataDir(), "packs", packName, "symlinks")
	intermediatePath := filepath.Join(intermediateDir, ".vimrc")
	s.Require().NoError(s.fs.MkdirAll(intermediateDir, 0755))
	s.Require().NoError(s.fs.Symlink(incorrectSourcePath, intermediatePath))

	// Call Link, which should correct the symlink
	_, err := ds.Link(packName, sourceFilePath)
	s.Require().NoError(err)

	// Verify the symlink now points to the correct source
	target, err := s.fs.Readlink(intermediatePath)
	s.Require().NoError(err)
	s.Equal(sourceFilePath, target)
}

func (s *DataStoreSuite) TestUnlink() {
	ds := datastore.New(s.fs, s.paths)
	packName := "vim"
	sourceFileDir := filepath.Join(s.paths.DotfilesRoot(), packName)
	sourceFilePath := filepath.Join(sourceFileDir, ".vimrc")
	s.Require().NoError(s.fs.MkdirAll(sourceFileDir, 0755))
	s.Require().NoError(s.fs.WriteFile(sourceFilePath, []byte("vim config"), 0644))

	// Create the link first
	intermediatePath, err := ds.Link(packName, sourceFilePath)
	s.Require().NoError(err)

	// Now, unlink it
	err = ds.Unlink(packName, sourceFilePath)
	s.Require().NoError(err)

	// Verify the link no longer exists
	_, err = s.fs.Lstat(intermediatePath)
	s.Require().Error(err)
	s.True(os.IsNotExist(err))
}

func (s *DataStoreSuite) TestAddToPath() {
	ds := datastore.New(s.fs, s.paths)
	packName := "tools"
	dirPath := filepath.Join(s.paths.DotfilesRoot(), packName, "bin")
	s.Require().NoError(s.fs.MkdirAll(dirPath, 0755))

	err := ds.AddToPath(packName, dirPath)
	s.Require().NoError(err)

	expectedIntermediatePath := filepath.Join(s.paths.DataDir(), "packs", packName, "path", "bin")
	target, err := s.fs.Readlink(expectedIntermediatePath)
	s.Require().NoError(err)
	s.Equal(dirPath, target)
}

func (s *DataStoreSuite) TestAddToShellProfile() {
	ds := datastore.New(s.fs, s.paths)
	packName := "git"
	scriptPath := filepath.Join(s.paths.DotfilesRoot(), packName, "aliases.sh")
	s.Require().NoError(s.fs.MkdirAll(filepath.Dir(scriptPath), 0755))
	s.Require().NoError(s.fs.WriteFile(scriptPath, []byte("alias g=git"), 0644))

	err := ds.AddToShellProfile(packName, scriptPath)
	s.Require().NoError(err)

	expectedIntermediatePath := filepath.Join(s.paths.DataDir(), "packs", packName, "shell_profile", "aliases.sh")
	target, err := s.fs.Readlink(expectedIntermediatePath)
	s.Require().NoError(err)
	s.Equal(scriptPath, target)
}

func (s *DataStoreSuite) TestRecordProvisioning() {
	ds := datastore.New(s.fs, s.paths)
	packName := "dev"
	sentinelName := "install.sh.sentinel"
	checksum := "sha256:12345"

	sentinelDir := s.paths.PackHandlerDir(packName, "sentinels")
	s.Require().NoError(s.fs.MkdirAll(sentinelDir, 0755))

	err := ds.RecordProvisioning(packName, sentinelName, checksum)
	s.Require().NoError(err)

	sentinelPath := filepath.Join(sentinelDir, sentinelName)
	content, err := s.fs.ReadFile(sentinelPath)
	s.Require().NoError(err)
	s.Contains(string(content), checksum)
}

func (s *DataStoreSuite) TestNeedsProvisioning() {
	ds := datastore.New(s.fs, s.paths)
	packName := "dev"
	sentinelName := "install.sh.sentinel"
	checksum := "sha256:12345"
	wrongChecksum := "sha256:67890"

	// 1. No sentinel file exists
	needs, err := ds.NeedsProvisioning(packName, sentinelName, checksum)
	s.Require().NoError(err)
	s.True(needs)

	// 2. Sentinel file exists with correct checksum
	sentinelDir := s.paths.PackHandlerDir(packName, "sentinels")
	s.Require().NoError(s.fs.MkdirAll(sentinelDir, 0755))
	s.Require().NoError(ds.RecordProvisioning(packName, sentinelName, checksum))
	needs, err = ds.NeedsProvisioning(packName, sentinelName, checksum)
	s.Require().NoError(err)
	s.False(needs)

	// 3. Sentinel file exists with incorrect checksum
	needs, err = ds.NeedsProvisioning(packName, sentinelName, wrongChecksum)
	s.Require().NoError(err)
	s.True(needs)
}

func (s *DataStoreSuite) TestGetStatus() {
	ds := datastore.New(s.fs, s.paths)
	packName := "vim"
	sourceFileDir := filepath.Join(s.paths.DotfilesRoot(), packName)
	sourceFilePath := filepath.Join(sourceFileDir, ".vimrc")
	s.Require().NoError(s.fs.MkdirAll(sourceFileDir, 0755))
	s.Require().NoError(s.fs.WriteFile(sourceFilePath, []byte("vim config"), 0644))

	// 1. Status when not linked
	status, err := ds.GetStatus(packName, sourceFilePath)
	s.Require().NoError(err)
	s.Equal(types.StatusStateMissing, status.State)

	// 2. Status when linked
	_, err = ds.Link(packName, sourceFilePath)
	s.Require().NoError(err)
	status, err = ds.GetStatus(packName, sourceFilePath)
	s.Require().NoError(err)
	s.Equal(types.StatusStateReady, status.State)
}
