package testutil

import (
	"path/filepath"
	"testing"
)

// PackFixture represents a test pack with files and directories
type PackFixture struct {
	Name  string
	Files map[string]string // path -> content
	Dirs  []string
}

// CreatePackFixture creates a pack fixture in the given directory
func CreatePackFixture(t *testing.T, baseDir string, fixture PackFixture) string {
	t.Helper()

	packDir := CreateDir(t, baseDir, fixture.Name)

	// Create directories
	for _, dir := range fixture.Dirs {
		CreateDir(t, packDir, dir)
	}

	// Create files
	for path, content := range fixture.Files {
		CreateFile(t, packDir, path, content)
	}

	return packDir
}

// CommonPackFixtures returns common pack fixtures for testing
func CommonPackFixtures() []PackFixture {
	return []PackFixture{
		{
			Name: "vim-pack",
			Files: map[string]string{
				".vimrc":        "\" Vim configuration\nset number\nset expandtab",
				".vim/init.vim": "\" Neovim config",
				"README.txxt":   "# Vim Configuration",
			},
			Dirs: []string{".vim/colors", ".vim/plugin"},
		},
		{
			Name: "shell-pack",
			Files: map[string]string{
				".zshrc":     "# Zsh configuration\nexport EDITOR=vim",
				".bashrc":    "# Bash configuration\nalias ll='ls -la'",
				"alias.sh":   "alias g='git'\nalias dc='docker-compose'",
				"exports.sh": "export PATH=$HOME/bin:$PATH",
			},
		},
		{
			Name: "bin-pack",
			Files: map[string]string{
				"bin/script1": "#!/bin/bash\necho 'Script 1'",
				"bin/script2": "#!/bin/bash\necho 'Script 2'",
			},
			Dirs: []string{"bin"},
		},
		{
			Name: "config-pack",
			Files: map[string]string{
				".dodot.toml": `# Pack configuration
[[matchers]]
trigger = "filename"
pattern = "*.conf"
handler = "symlink"
target = "$HOME/.config"
`,
				"app.conf": "# Application config",
			},
		},
	}
}

// CreateDotfilesRepo creates a complete test dotfiles repository
func CreateDotfilesRepo(t *testing.T) string {
	t.Helper()

	repoDir := TempDir(t, "dodot-test-repo")

	// Create common pack fixtures
	for _, fixture := range CommonPackFixtures() {
		CreatePackFixture(t, repoDir, fixture)
	}

	// Create a .gitignore
	CreateFile(t, repoDir, ".gitignore", "*.log\n.DS_Store\n")

	return repoDir
}

// CreateMinimalPack creates a minimal pack for simple tests
func CreateMinimalPack(t *testing.T, baseDir, packName string) string {
	t.Helper()

	fixture := PackFixture{
		Name: packName,
		Files: map[string]string{
			"test.txt": "test content",
		},
	}

	return CreatePackFixture(t, baseDir, fixture)
}

// CreateComplexPack creates a pack with various file types for comprehensive testing
func CreateComplexPack(t *testing.T, baseDir string) string {
	t.Helper()

	fixture := PackFixture{
		Name: "complex-pack",
		Files: map[string]string{
			// Dotfiles
			".config1": "config 1",
			".config2": "config 2",

			// Regular files
			"README.txxt": "# Complex Pack",
			"install.sh":  "#!/bin/bash\necho 'Installing...'",
			"Brewfile":    "brew 'git'\nbrew 'vim'",

			// Nested structure
			"config/app.yml":        "app: config",
			"scripts/deploy.sh":     "#!/bin/bash\necho 'Deploying...'",
			"templates/config.tmpl": "{{ .Variable }}",

			// Binary directory
			"bin/tool1": "#!/bin/bash\necho 'Tool 1'",
			"bin/tool2": "#!/bin/bash\necho 'Tool 2'",

			// Shell configuration
			"alias.sh":   "alias x='exit'",
			"exports.sh": "export COMPLEX=true",
			"path.sh":    "export PATH=$PATH:$HOME/.local/bin",

			// Pack configuration
			".dodot.toml": `# Complex pack configuration
[pack]
description = "A complex test pack"
priority = 10

[[matchers]]
trigger = "filename"
pattern = ".config*"
handler = "symlink"

[[matchers]]
trigger = "directory"
pattern = "bin"
handler = "bin"
`,
		},
		Dirs: []string{
			"config",
			"scripts",
			"templates",
			"bin",
			"empty-dir",
		},
	}

	packDir := CreatePackFixture(t, baseDir, fixture)

	// Make scripts executable
	Chmod(t, filepath.Join(packDir, "install.sh"), 0755)
	Chmod(t, filepath.Join(packDir, "scripts/deploy.sh"), 0755)
	Chmod(t, filepath.Join(packDir, "bin/tool1"), 0755)
	Chmod(t, filepath.Join(packDir, "bin/tool2"), 0755)

	return packDir
}
