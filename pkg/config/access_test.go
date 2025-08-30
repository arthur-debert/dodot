// Test Type: Unit Test
// Description: Tests for the config package - global configuration access functions

package config_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestInitialize(t *testing.T) {
	tests := []struct {
		name   string
		config *config.Config
		verify func(t *testing.T)
	}{
		{
			name:   "initialize_with_nil_loads_default",
			config: nil,
			verify: func(t *testing.T) {
				cfg := config.Get()
				assert.NotNil(t, cfg)
				// Should have some default values
				assert.NotEmpty(t, cfg.Rules)
				assert.NotEmpty(t, cfg.Security.ProtectedPaths)
				assert.NotEmpty(t, cfg.Patterns.PackIgnore)
			},
		},
		{
			name: "initialize_with_custom_config",
			config: &config.Config{
				Security: config.Security{
					ProtectedPaths: map[string]bool{
						"/custom/protected": true,
					},
				},
				Patterns: config.Patterns{
					PackIgnore: []string{"*.custom"},
					SpecialFiles: config.SpecialFiles{
						PackConfig: "custom.toml",
						IgnoreFile: ".customignore",
					},
				},
			},
			verify: func(t *testing.T) {
				cfg := config.Get()
				assert.True(t, cfg.Security.ProtectedPaths["/custom/protected"])
				assert.Contains(t, cfg.Patterns.PackIgnore, "*.custom")
				assert.Equal(t, "custom.toml", cfg.Patterns.SpecialFiles.PackConfig)
				assert.Equal(t, ".customignore", cfg.Patterns.SpecialFiles.IgnoreFile)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Reset global config between tests
			config.Initialize(nil)

			// Initialize with test config
			config.Initialize(tt.config)

			// Verify
			tt.verify(t)
		})
	}
}

func TestGet_LazyInitialization(t *testing.T) {
	// Force reset by initializing with a known config
	testConfig := &config.Config{
		Patterns: config.Patterns{
			PackIgnore: []string{"test-pattern"},
		},
	}
	config.Initialize(testConfig)

	// Now test that Get() returns the initialized config
	cfg := config.Get()
	require.NotNil(t, cfg)
	assert.Contains(t, cfg.Patterns.PackIgnore, "test-pattern")
}

func TestGetPaths(t *testing.T) {
	// Initialize with known config (Paths is empty struct for now)
	config.Initialize(&config.Config{
		Paths: config.Paths{},
	})

	paths := config.GetPaths()
	assert.NotNil(t, paths)
	// Paths struct is currently empty, reserved for future use
}

func TestGetSecurity(t *testing.T) {
	config.Initialize(&config.Config{
		Security: config.Security{
			ProtectedPaths: map[string]bool{
				"/etc/passwd": true,
				"/etc/shadow": true,
				".ssh/id_rsa": true,
			},
		},
	})

	security := config.GetSecurity()
	assert.True(t, security.ProtectedPaths["/etc/passwd"])
	assert.True(t, security.ProtectedPaths["/etc/shadow"])
	assert.True(t, security.ProtectedPaths[".ssh/id_rsa"])
}

func TestGetPatterns(t *testing.T) {
	config.Initialize(&config.Config{
		Patterns: config.Patterns{
			PackIgnore:      []string{"*.tmp", "*.cache"},
			CatchallExclude: []string{".dodot.toml", ".dodotignore"},
			SpecialFiles: config.SpecialFiles{
				PackConfig: ".dodot.toml",
				IgnoreFile: ".dodotignore",
			},
		},
	})

	patterns := config.GetPatterns()
	assert.Equal(t, []string{"*.tmp", "*.cache"}, patterns.PackIgnore)
	assert.Equal(t, []string{".dodot.toml", ".dodotignore"}, patterns.CatchallExclude)
	assert.Equal(t, ".dodot.toml", patterns.SpecialFiles.PackConfig)
	assert.Equal(t, ".dodotignore", patterns.SpecialFiles.IgnoreFile)
}

func TestGetFilePermissions(t *testing.T) {
	config.Initialize(&config.Config{
		FilePermissions: config.FilePermissions{
			File:       0644,
			Executable: 0755,
			Directory:  0755,
		},
	})

	perms := config.GetFilePermissions()
	assert.Equal(t, 0644, int(perms.File))
	assert.Equal(t, 0755, int(perms.Executable))
	assert.Equal(t, 0755, int(perms.Directory))
}

func TestGetShellIntegration(t *testing.T) {
	config.Initialize(&config.Config{
		ShellIntegration: config.ShellIntegration{
			BashZshSnippet:           `source "$HOME/.local/share/dodot/shell/init.sh"`,
			BashZshSnippetWithCustom: `source "%s/shell/init.sh"`,
			FishSnippet:              `source "$HOME/.local/share/dodot/shell/init.fish"`,
		},
	})

	shell := config.GetShellIntegration()
	assert.Equal(t, `source "$HOME/.local/share/dodot/shell/init.sh"`, shell.BashZshSnippet)
	assert.Equal(t, `source "%s/shell/init.sh"`, shell.BashZshSnippetWithCustom)
	assert.Equal(t, `source "$HOME/.local/share/dodot/shell/init.fish"`, shell.FishSnippet)
}

func TestGetLinkPaths(t *testing.T) {
	config.Initialize(&config.Config{
		LinkPaths: config.LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"ssh":    true,
				"gnupg":  true,
				"bashrc": true,
			},
		},
	})

	links := config.GetLinkPaths()
	assert.True(t, links.CoreUnixExceptions["ssh"])
	assert.True(t, links.CoreUnixExceptions["gnupg"])
	assert.True(t, links.CoreUnixExceptions["bashrc"])
	assert.False(t, links.CoreUnixExceptions["unknown"])
}

func TestGetRules(t *testing.T) {
	testRules := []config.Rule{
		{
			Pattern: "*.sh",
			Handler: "shell",
			Options: map[string]interface{}{
				"placement": "append",
			},
		},
		{
			Pattern: "*.conf",
			Handler: "symlink",
			Options: map[string]interface{}{
				"force": true,
			},
		},
	}

	config.Initialize(&config.Config{
		Rules: testRules,
	})

	rules := config.GetRules()
	assert.Len(t, rules, 2)
	assert.Equal(t, "*.sh", rules[0].Pattern)
	assert.Equal(t, "shell", rules[0].Handler)
	assert.Equal(t, "*.conf", rules[1].Pattern)
	assert.Equal(t, "symlink", rules[1].Handler)
}

// Test that all getter functions work together
func TestAllGetters_Integration(t *testing.T) {
	// Initialize once with a complete config
	config.Initialize(&config.Config{
		Security: config.Security{
			ProtectedPaths: map[string]bool{
				".integrated": true,
			},
		},
		Patterns: config.Patterns{
			PackIgnore: []string{"*.integrated"},
			SpecialFiles: config.SpecialFiles{
				PackConfig: "integrated.toml",
			},
		},
		FilePermissions: config.FilePermissions{
			File: 0600,
		},
		ShellIntegration: config.ShellIntegration{
			BashZshSnippet: "integrated",
		},
		LinkPaths: config.LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"integrated": true,
			},
		},
		Rules: []config.Rule{
			{
				Pattern: "integrated.*",
				Handler: "test",
			},
		},
	})

	// Call all getters and verify they return consistent data
	assert.True(t, config.GetSecurity().ProtectedPaths[".integrated"])
	assert.Equal(t, []string{"*.integrated"}, config.GetPatterns().PackIgnore)
	assert.Equal(t, 0600, int(config.GetFilePermissions().File))
	assert.Equal(t, "integrated", config.GetShellIntegration().BashZshSnippet)
	assert.True(t, config.GetLinkPaths().CoreUnixExceptions["integrated"])
	assert.Len(t, config.GetRules(), 1)
	assert.Equal(t, "integrated.*", config.GetRules()[0].Pattern)
}
