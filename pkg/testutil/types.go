package testutil

import (
	"fmt"
	"path/filepath"
	"strings"
)

// FileTree represents a nested file structure for declarative test setup
type FileTree map[string]interface{}

// PackConfig defines the configuration for setting up a test pack
type PackConfig struct {
	// Files maps relative paths to file contents
	Files map[string]string
	
	// Rules defines the dodot rules for this pack
	Rules []Rule
	
	// Directories to create (without files)
	Dirs []string
}

// Rule represents a dodot rule configuration
type Rule struct {
	Type    string // "filename", "glob", "directory"
	Pattern string
	Handler string
	Options map[string]interface{}
}

// String converts a Rule to TOML format
func (r Rule) String() string {
	parts := []string{
		fmt.Sprintf("[[rules]]"),
		fmt.Sprintf("type = %q", r.Type),
		fmt.Sprintf("pattern = %q", r.Pattern),
		fmt.Sprintf("handler = %q", r.Handler),
	}
	
	if len(r.Options) > 0 {
		parts = append(parts, "[rules.options]")
		for k, v := range r.Options {
			parts = append(parts, fmt.Sprintf("%s = %q", k, v))
		}
	}
	
	return strings.Join(parts, "\n")
}

// TestPack represents a pack created in the test environment
type TestPack struct {
	Name string
	Path string
	env  *TestEnvironment
}

// AddFile adds a file to an existing test pack
func (p *TestPack) AddFile(path, content string) *TestPack {
	fullPath := filepath.Join(p.Path, path)
	dir := filepath.Dir(fullPath)
	
	// Create parent directories
	if err := p.env.FS.MkdirAll(dir, 0755); err != nil {
		p.env.t.Fatalf("failed to create directory %s: %v", dir, err)
	}
	
	// Write file
	if err := p.env.FS.WriteFile(fullPath, []byte(content), 0644); err != nil {
		p.env.t.Fatalf("failed to write file %s: %v", fullPath, err)
	}
	
	return p
}

// AddSymlink adds a symlink to the test pack
func (p *TestPack) AddSymlink(source, target string) *TestPack {
	sourcePath := filepath.Join(p.Path, source)
	
	if err := p.env.FS.Symlink(target, sourcePath); err != nil {
		p.env.t.Fatalf("failed to create symlink %s -> %s: %v", sourcePath, target, err)
	}
	
	return p
}

// AddDirectory creates a directory in the test pack
func (p *TestPack) AddDirectory(path string) *TestPack {
	dirPath := filepath.Join(p.Path, path)
	
	if err := p.env.FS.MkdirAll(dirPath, 0755); err != nil {
		p.env.t.Fatalf("failed to create directory %s: %v", dirPath, err)
	}
	
	return p
}

// Common pre-built pack configurations

// VimPack returns a standard vim pack configuration
func VimPack() PackConfig {
	return PackConfig{
		Files: map[string]string{
			"vimrc":           "\" Standard vimrc\nset number\nset expandtab",
			"gvimrc":          "\" GUI vim config\nset guifont=Monaco:h12",
			"colors/monokai.vim": "\" Monokai color scheme",
		},
		Rules: []Rule{
			{Type: "filename", Pattern: ".*rc$", Handler: "symlink"},
			{Type: "directory", Pattern: "colors", Handler: "symlink"},
		},
	}
}

// GitPack returns a standard git pack configuration
func GitPack() PackConfig {
	return PackConfig{
		Files: map[string]string{
			"gitconfig": "[user]\n  name = Test User\n  email = test@example.com",
			"gitignore": "*.log\n.DS_Store\nnode_modules/",
		},
		Rules: []Rule{
			{Type: "filename", Pattern: "gitconfig", Handler: "symlink"},
			{Type: "filename", Pattern: "gitignore", Handler: "symlink"},
		},
	}
}

// ShellPack returns a standard shell pack configuration
func ShellPack() PackConfig {
	return PackConfig{
		Files: map[string]string{
			"aliases.sh": "alias ll='ls -la'\nalias gs='git status'",
			"exports.sh": "export EDITOR=vim\nexport PATH=$HOME/bin:$PATH",
			"bashrc":     "source ~/.dotfiles/shell/aliases.sh",
			"zshrc":      "source ~/.dotfiles/shell/aliases.sh",
		},
		Rules: []Rule{
			{Type: "glob", Pattern: "*.sh", Handler: "shell"},
			{Type: "filename", Pattern: "bashrc", Handler: "symlink"},
			{Type: "filename", Pattern: "zshrc", Handler: "symlink"},
		},
	}
}

// ToolsPack returns a pack with install scripts and homebrew
func ToolsPack() PackConfig {
	return PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/bash\necho 'Installing tools'\nexit 0",
			"Brewfile": "brew 'ripgrep'\nbrew 'fd'\ncask 'visual-studio-code'",
		},
		Rules: []Rule{
			{Type: "filename", Pattern: "install.sh", Handler: "install"},
			{Type: "filename", Pattern: "Brewfile", Handler: "homebrew"},
		},
	}
}