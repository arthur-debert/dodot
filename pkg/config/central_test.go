package config

import (
	"os"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestGenerateRulesFromMapping(t *testing.T) {
	tests := []struct {
		name     string
		mappings Mappings
		expected int // expected number of rules
		validate func(t *testing.T, rules []Rule)
	}{
		{
			name:     "empty file mapping",
			mappings: Mappings{},
			expected: 0,
		},
		{
			name: "path mapping only",
			mappings: Mappings{
				Path: "bin",
			},
			expected: 1,
			validate: func(t *testing.T, rules []Rule) {
				assert.Equal(t, "bin/", rules[0].Pattern)
				assert.Equal(t, "path", rules[0].Handler)
			},
		},
		{
			name: "install mapping only",
			mappings: Mappings{
				Install: "setup.sh",
			},
			expected: 1,
			validate: func(t *testing.T, rules []Rule) {
				assert.Equal(t, "setup.sh", rules[0].Pattern)
				assert.Equal(t, "install", rules[0].Handler)
			},
		},
		{
			name: "shell mappings with placement detection",
			mappings: Mappings{
				Shell: []string{"*.sh", "*aliases.sh", "login.sh"},
			},
			expected: 3,
			validate: func(t *testing.T, rules []Rule) {
				assert.Equal(t, "*.sh", rules[0].Pattern)
				assert.Equal(t, "shell", rules[0].Handler)
				assert.Equal(t, "environment", rules[0].Options["placement"])

				assert.Equal(t, "*aliases.sh", rules[1].Pattern)
				assert.Equal(t, "shell", rules[1].Handler)
				assert.Equal(t, "aliases", rules[1].Options["placement"])

				assert.Equal(t, "login.sh", rules[2].Pattern)
				assert.Equal(t, "shell", rules[2].Handler)
				assert.Equal(t, "login", rules[2].Options["placement"])
			},
		},
		{
			name: "homebrew mapping only",
			mappings: Mappings{
				Homebrew: "Brewfile",
			},
			expected: 1,
			validate: func(t *testing.T, rules []Rule) {
				assert.Equal(t, "Brewfile", rules[0].Pattern)
				assert.Equal(t, "homebrew", rules[0].Handler)
			},
		},
		{
			name: "all mappings",
			mappings: Mappings{
				Path:     "bin",
				Install:  "install.sh",
				Shell:    []string{"*.sh"},
				Homebrew: "Brewfile",
			},
			expected: 4,
			validate: func(t *testing.T, rules []Rule) {
				assert.Equal(t, 4, len(rules))
				// Check path rule
				assert.Equal(t, "bin/", rules[0].Pattern)
				assert.Equal(t, "path", rules[0].Handler)
				// Check install rule
				assert.Equal(t, "install.sh", rules[1].Pattern)
				assert.Equal(t, "install", rules[1].Handler)
				// Check shell rule
				assert.Equal(t, "*.sh", rules[2].Pattern)
				assert.Equal(t, "shell", rules[2].Handler)
				// Check homebrew rule
				assert.Equal(t, "Brewfile", rules[3].Pattern)
				assert.Equal(t, "homebrew", rules[3].Handler)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := &Config{
				Mappings: tt.mappings,
			}
			rules := cfg.GenerateRulesFromMapping()

			assert.Len(t, rules, tt.expected, "expected %d rules, got %d", tt.expected, len(rules))

			if tt.validate != nil {
				tt.validate(t, rules)
			}
		})
	}
}

func TestDefaultRules(t *testing.T) {
	rules := defaultRules()

	// Check we have rules
	assert.NotEmpty(t, rules)

	// Count exclusion rules
	exclusionCount := 0
	for _, r := range rules {
		if len(r.Pattern) > 0 && r.Pattern[0] == '!' {
			exclusionCount++
		}
	}
	assert.Greater(t, exclusionCount, 0, "Should have exclusion rules")

	// Check for essential rules
	patterns := make(map[string]string)
	for _, r := range rules {
		if r.Handler != "" {
			patterns[r.Pattern] = r.Handler
		}
	}

	assert.Equal(t, "install", patterns["install.sh"])
	assert.Equal(t, "homebrew", patterns["Brewfile"])
	assert.Equal(t, "symlink", patterns["*"])
	assert.Equal(t, "path", patterns["bin/"])
}

func TestDefault(t *testing.T) {
	cfg := Default()

	// Test Security
	assert.NotNil(t, cfg.Security.ProtectedPaths)
	assert.True(t, cfg.Security.ProtectedPaths[".ssh/authorized_keys"])

	// Test Rules
	assert.NotEmpty(t, cfg.Rules)

	// Test Patterns
	assert.NotEmpty(t, cfg.Patterns.PackIgnore)
	assert.Equal(t, ".dodot.toml", cfg.Patterns.SpecialFiles.PackConfig)
	assert.Equal(t, ".dodotignore", cfg.Patterns.SpecialFiles.IgnoreFile)

	// Test FilePermissions
	assert.Equal(t, os.FileMode(0755), cfg.FilePermissions.Directory)
	assert.Equal(t, os.FileMode(0644), cfg.FilePermissions.File)
	assert.Equal(t, os.FileMode(0755), cfg.FilePermissions.Executable)

	// Test ShellIntegration
	assert.NotEmpty(t, cfg.ShellIntegration.BashZshSnippet)
	assert.NotEmpty(t, cfg.ShellIntegration.FishSnippet)

	// Test LinkPaths
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["ssh"])
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["gitconfig"])
}
