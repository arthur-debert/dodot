// pkg/testutil/environment_test.go
// TEST TYPE: Unit Test
// DEPENDENCIES: None
// PURPOSE: Test TestEnvironment orchestration

package testutil

import (
	"os"
	"path/filepath"
	"testing"
)

func TestTestEnvironment_MemoryOnly(t *testing.T) {
	env := NewTestEnvironment(t, EnvMemoryOnly)
	
	// Test environment paths are set
	if env.DotfilesRoot == "" {
		t.Error("DotfilesRoot not set")
	}
	if env.HomeDir == "" {
		t.Error("HomeDir not set")
	}
	
	// Test filesystem operations
	testFile := filepath.Join(env.DotfilesRoot, "test.txt")
	err := env.FS.WriteFile(testFile, []byte("test"), 0644)
	if err != nil {
		t.Fatalf("WriteFile failed: %v", err)
	}
	
	content, err := env.FS.ReadFile(testFile)
	if err != nil {
		t.Fatalf("ReadFile failed: %v", err)
	}
	
	if string(content) != "test" {
		t.Errorf("content mismatch: got %q, want %q", content, "test")
	}
	
	// Test environment variables are set
	if os.Getenv("DOTFILES_ROOT") != env.DotfilesRoot {
		t.Error("DOTFILES_ROOT env var not set correctly")
	}
	if os.Getenv("HOME") != env.HomeDir {
		t.Error("HOME env var not set correctly")
	}
}

func TestTestEnvironment_SetupPack(t *testing.T) {
	env := NewTestEnvironment(t, EnvMemoryOnly)
	
	// Setup a test pack
	pack := env.SetupPack("vim", PackConfig{
		Files: map[string]string{
			"vimrc":  "set number",
			"gvimrc": "set guifont",
		},
		Rules: []Rule{
			{Type: "filename", Pattern: ".*rc", Handler: "symlink"},
		},
	})
	
	// Verify pack was created
	if pack.Name != "vim" {
		t.Errorf("pack name wrong: got %q, want %q", pack.Name, "vim")
	}
	
	// Verify files exist
	vimrcPath := filepath.Join(pack.Path, "vimrc")
	content, err := env.FS.ReadFile(vimrcPath)
	if err != nil {
		t.Fatalf("couldn't read vimrc: %v", err)
	}
	if string(content) != "set number" {
		t.Errorf("vimrc content wrong: got %q", content)
	}
	
	// Verify rules file exists
	rulesPath := filepath.Join(pack.Path, ".dodot.toml")
	rulesContent, err := env.FS.ReadFile(rulesPath)
	if err != nil {
		t.Fatalf("couldn't read rules file: %v", err)
	}
	if len(rulesContent) == 0 {
		t.Error("rules file is empty")
	}
}

func TestTestEnvironment_WithFileTree(t *testing.T) {
	env := NewTestEnvironment(t, EnvMemoryOnly)
	
	// Setup file tree
	env.WithFileTree(FileTree{
		"vim": FileTree{
			"vimrc": "vim config",
			"colors": FileTree{
				"monokai.vim": "color scheme",
			},
		},
		"git": FileTree{
			"gitconfig": "[user]\n  name = Test",
		},
	})
	
	// Verify vim files
	vimrcPath := filepath.Join(env.DotfilesRoot, "vim", "vimrc")
	content, err := env.FS.ReadFile(vimrcPath)
	if err != nil {
		t.Fatalf("couldn't read vimrc: %v", err)
	}
	if string(content) != "vim config" {
		t.Errorf("vimrc content wrong: got %q", content)
	}
	
	// Verify nested file
	colorPath := filepath.Join(env.DotfilesRoot, "vim", "colors", "monokai.vim")
	content, err = env.FS.ReadFile(colorPath)
	if err != nil {
		t.Fatalf("couldn't read color scheme: %v", err)
	}
	if string(content) != "color scheme" {
		t.Errorf("color scheme content wrong: got %q", content)
	}
	
	// Verify git files
	gitPath := filepath.Join(env.DotfilesRoot, "git", "gitconfig")
	content, err = env.FS.ReadFile(gitPath)
	if err != nil {
		t.Fatalf("couldn't read gitconfig: %v", err)
	}
	if string(content) != "[user]\n  name = Test" {
		t.Errorf("gitconfig content wrong: got %q", content)
	}
}

func TestTestEnvironment_PreBuiltPacks(t *testing.T) {
	env := NewTestEnvironment(t, EnvMemoryOnly)
	
	// Test VimPack
	t.Run("VimPack", func(t *testing.T) {
		pack := env.SetupPack("vim", VimPack())
		
		// Check files
		files := []string{"vimrc", "gvimrc", "colors/monokai.vim"}
		for _, file := range files {
			path := filepath.Join(pack.Path, file)
			if _, err := env.FS.Stat(path); err != nil {
				t.Errorf("file %s doesn't exist: %v", file, err)
			}
		}
	})
	
	// Test GitPack
	t.Run("GitPack", func(t *testing.T) {
		pack := env.SetupPack("git", GitPack())
		
		// Check files
		files := []string{"gitconfig", "gitignore"}
		for _, file := range files {
			path := filepath.Join(pack.Path, file)
			if _, err := env.FS.Stat(path); err != nil {
				t.Errorf("file %s doesn't exist: %v", file, err)
			}
		}
	})
	
	// Test ToolsPack
	t.Run("ToolsPack", func(t *testing.T) {
		pack := env.SetupPack("tools", ToolsPack())
		
		// Check files
		files := []string{"install.sh", "Brewfile"}
		for _, file := range files {
			path := filepath.Join(pack.Path, file)
			if _, err := env.FS.Stat(path); err != nil {
				t.Errorf("file %s doesn't exist: %v", file, err)
			}
		}
	})
}