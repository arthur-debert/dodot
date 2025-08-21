package testutil

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSetupTestPack(t *testing.T) {
	pack := SetupTestPack(t, "mypack")

	assert.Contains(t, pack.Root, "dotfiles")
	assert.Equal(t, "mypack", pack.Name)
	assert.Equal(t, filepath.Join(pack.Root, "mypack"), pack.Dir)

	// Verify directories were created
	info, err := os.Stat(pack.Dir)
	require.NoError(t, err)
	assert.True(t, info.IsDir())
}

func TestSetupTestPackWithHome(t *testing.T) {
	origHome := os.Getenv("HOME")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	pack, homeDir := SetupTestPackWithHome(t, "mypack")

	assert.Equal(t, homeDir, os.Getenv("HOME"))

	// Verify home directory was created
	info, err := os.Stat(homeDir)
	require.NoError(t, err)
	assert.True(t, info.IsDir())

	// Verify pack was created
	assert.Equal(t, "mypack", pack.Name)
}

func TestTestPack_AddFile(t *testing.T) {
	pack := SetupTestPack(t, "mypack")

	filePath := pack.AddFile(t, "test.txt", "test content")

	assert.Equal(t, filepath.Join(pack.Dir, "test.txt"), filePath)

	// Verify file was created with correct content
	content, err := os.ReadFile(filePath)
	require.NoError(t, err)
	assert.Equal(t, "test content", string(content))

	// Verify permissions
	info, err := os.Stat(filePath)
	require.NoError(t, err)
	assert.Equal(t, os.FileMode(0644), info.Mode().Perm())
}

func TestTestPack_AddExecutable(t *testing.T) {
	pack := SetupTestPack(t, "mypack")

	filePath := pack.AddExecutable(t, "script.sh", "#!/bin/bash\necho test")

	// Verify permissions
	info, err := os.Stat(filePath)
	require.NoError(t, err)
	assert.Equal(t, os.FileMode(0755), info.Mode().Perm())
}

func TestTestPack_AddDodotConfig(t *testing.T) {
	pack := SetupTestPack(t, "mypack")

	config := `[[rules]]
trigger = "filename"
pattern = ".*"
handler = "symlink"
`
	pack.AddDodotConfig(t, config)

	// Verify config file was created
	configPath := filepath.Join(pack.Dir, ".dodot.toml")
	content, err := os.ReadFile(configPath)
	require.NoError(t, err)
	assert.Equal(t, config, string(content))
}

func TestTestPack_AddSymlinkRule(t *testing.T) {
	pack := SetupTestPack(t, "mypack")

	pack.AddSymlinkRule(t, ".vimrc")

	// Verify config was created with correct pattern
	configPath := filepath.Join(pack.Dir, ".dodot.toml")
	content, err := os.ReadFile(configPath)
	require.NoError(t, err)
	assert.Contains(t, string(content), `pattern = ".vimrc"`)
	assert.Contains(t, string(content), `handler = "symlink"`)
}

func TestSetupMultiplePacks(t *testing.T) {
	packs := SetupMultiplePacks(t, "vim", "bash", "git")

	assert.Len(t, packs, 3)
	assert.Contains(t, packs, "vim")
	assert.Contains(t, packs, "bash")
	assert.Contains(t, packs, "git")

	// Verify all packs share the same root
	var root string
	for _, pack := range packs {
		if root == "" {
			root = pack.Root
		} else {
			assert.Equal(t, root, pack.Root)
		}
	}

	// Verify all directories were created
	for name, pack := range packs {
		assert.Equal(t, name, pack.Name)
		info, err := os.Stat(pack.Dir)
		require.NoError(t, err)
		assert.True(t, info.IsDir())
	}
}

func TestTestPack_AddCommonDotfile(t *testing.T) {
	pack := SetupTestPack(t, "mypack")

	// Test known dotfile
	vimrcPath := pack.AddCommonDotfile(t, ".vimrc")
	content, err := os.ReadFile(vimrcPath)
	require.NoError(t, err)
	assert.Contains(t, string(content), "vim configuration")

	// Test unknown dotfile
	unknownPath := pack.AddCommonDotfile(t, ".unknown")
	content, err = os.ReadFile(unknownPath)
	require.NoError(t, err)
	assert.Equal(t, "# .unknown test content", string(content))
}

func TestTestPack_AddStandardConfig(t *testing.T) {
	pack := SetupTestPack(t, "mypack")

	pack.AddStandardConfig(t, "symlink")

	// Verify config was created
	configPath := filepath.Join(pack.Dir, ".dodot.toml")
	content, err := os.ReadFile(configPath)
	require.NoError(t, err)
	assert.Contains(t, string(content), `pattern = ".*"`)
	assert.Contains(t, string(content), `handler = "symlink"`)
}

func TestTestPack_AddStandardConfig_UnknownType(t *testing.T) {
	// We can't easily test t.Fatal, so we'll skip this test
	// The function will cause the test to fail if called with unknown type
	t.Skip("Cannot test t.Fatal easily")
}
