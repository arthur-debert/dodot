package testutil

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

// TestPack represents a test pack with its directory structure
type TestPack struct {
	Root string // Dotfiles root directory
	Name string // Pack name
	Dir  string // Full path to pack directory
}

// SetupTestPack creates a basic test pack structure
func SetupTestPack(t *testing.T, packName string) *TestPack {
	t.Helper()

	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	packDir := filepath.Join(dotfilesRoot, packName)

	require.NoError(t, os.MkdirAll(packDir, 0755))

	return &TestPack{
		Root: dotfilesRoot,
		Name: packName,
		Dir:  packDir,
	}
}

// SetupTestPackWithHome creates a test pack with home directory
func SetupTestPackWithHome(t *testing.T, packName string) (*TestPack, string) {
	t.Helper()

	pack := SetupTestPack(t, packName)
	homeDir := filepath.Join(filepath.Dir(pack.Root), "home")

	require.NoError(t, os.MkdirAll(homeDir, 0755))
	t.Setenv("HOME", homeDir)

	return pack, homeDir
}

// AddFile adds a file to the test pack
func (tp *TestPack) AddFile(t *testing.T, filename, content string) string {
	t.Helper()

	filePath := filepath.Join(tp.Dir, filename)
	require.NoError(t, os.WriteFile(filePath, []byte(content), 0644))
	return filePath
}

// AddExecutable adds an executable file to the test pack
func (tp *TestPack) AddExecutable(t *testing.T, filename, content string) string {
	t.Helper()

	filePath := filepath.Join(tp.Dir, filename)
	require.NoError(t, os.WriteFile(filePath, []byte(content), 0755))
	return filePath
}

// AddDodotConfig adds a .dodot.toml configuration to the test pack
func (tp *TestPack) AddDodotConfig(t *testing.T, config string) {
	t.Helper()
	tp.AddFile(t, ".dodot.toml", config)
}

// AddSymlinkRule adds a standard symlink rule to the pack's config
func (tp *TestPack) AddSymlinkRule(t *testing.T, pattern string) {
	t.Helper()

	config := `[[rules]]
trigger = "filename"
pattern = "` + pattern + `"
handler = "symlink"
`
	tp.AddDodotConfig(t, config)
}

// SetupMultiplePacks creates multiple test packs at once
func SetupMultiplePacks(t *testing.T, packNames ...string) map[string]*TestPack {
	t.Helper()

	if len(packNames) == 0 {
		return nil
	}

	// Use the same root for all packs
	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")

	packs := make(map[string]*TestPack)
	for _, name := range packNames {
		packDir := filepath.Join(dotfilesRoot, name)
		require.NoError(t, os.MkdirAll(packDir, 0755))

		packs[name] = &TestPack{
			Root: dotfilesRoot,
			Name: name,
			Dir:  packDir,
		}
	}

	return packs
}

// CommonDotfiles provides common dotfile content for testing
var CommonDotfiles = map[string]string{
	".vimrc":     `" vim configuration\nset number\nset autoindent`,
	".bashrc":    `# bash configuration\nexport PS1="$ "\nalias ll="ls -la"`,
	".zshrc":     `# zsh configuration\nexport PS1="$ "\nalias ll="ls -la"`,
	".gitconfig": `[user]\n    name = Test User\n    email = test@example.com`,
}

// AddCommonDotfile adds a common dotfile with standard content
func (tp *TestPack) AddCommonDotfile(t *testing.T, filename string) string {
	t.Helper()

	content, ok := CommonDotfiles[filename]
	if !ok {
		content = "# " + filename + " test content"
	}

	return tp.AddFile(t, filename, content)
}

// StandardPackConfig provides standard pack configurations
var StandardPackConfig = map[string]string{
	"symlink": `[[rules]]
trigger = "filename"
pattern = ".*"
handler = "symlink"
`,
	"homebrew": `[[rules]]
trigger = "filename"  
pattern = "Brewfile"
handler = "homebrew"
`,
	"provision": `[[rules]]
trigger = "filename"
pattern = "install.sh"
handler = "install_script"
`,
}

// AddStandardConfig adds a standard configuration for common scenarios
func (tp *TestPack) AddStandardConfig(t *testing.T, configType string) {
	t.Helper()

	config, ok := StandardPackConfig[configType]
	if !ok {
		t.Fatalf("Unknown standard config type: %s", configType)
	}

	tp.AddDodotConfig(t, config)
}
