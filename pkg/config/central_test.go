// Test Type: Unit Test
// Description: Tests for the config package - central configuration structures and defaults

package config_test

import (
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/stretchr/testify/assert"
)

func TestDefault(t *testing.T) {
	cfg := config.Default()

	// Test security settings
	assert.NotEmpty(t, cfg.Security.ProtectedPaths)
	assert.True(t, cfg.Security.ProtectedPaths[".ssh/authorized_keys"])
	assert.True(t, cfg.Security.ProtectedPaths[".ssh/id_rsa"])
	assert.True(t, cfg.Security.ProtectedPaths[".gnupg"])

	// Test patterns
	assert.NotEmpty(t, cfg.Patterns.PackIgnore)
	assert.Contains(t, cfg.Patterns.PackIgnore, ".git")
	assert.Contains(t, cfg.Patterns.PackIgnore, ".DS_Store")
	assert.NotEmpty(t, cfg.Patterns.CatchallExclude)
	assert.Equal(t, ".dodot.toml", cfg.Patterns.SpecialFiles.PackConfig)
	assert.Equal(t, ".dodotignore", cfg.Patterns.SpecialFiles.IgnoreFile)

	// Test file permissions
	assert.Equal(t, os.FileMode(0644), cfg.FilePermissions.File)
	assert.Equal(t, os.FileMode(0755), cfg.FilePermissions.Executable)
	assert.Equal(t, os.FileMode(0755), cfg.FilePermissions.Directory)

	// Test shell integration
	assert.NotEmpty(t, cfg.ShellIntegration.BashZshSnippet)
	assert.NotEmpty(t, cfg.ShellIntegration.BashZshSnippetWithCustom)
	assert.NotEmpty(t, cfg.ShellIntegration.FishSnippet)
	assert.Contains(t, cfg.ShellIntegration.BashZshSnippet, "dodot-init.sh")
	assert.Contains(t, cfg.ShellIntegration.BashZshSnippetWithCustom, "%s")
	assert.Contains(t, cfg.ShellIntegration.FishSnippet, "dodot-init.fish")

	// Test link paths
	assert.NotEmpty(t, cfg.LinkPaths.CoreUnixExceptions)
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["ssh"])
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["bashrc"])
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["zshrc"])

	// Test rules exist
	assert.NotEmpty(t, cfg.Rules)
	// Check for some expected rules
	hasInstallRule := false
	hasBrewfileRule := false
	hasCatchallRule := false
	for _, rule := range cfg.Rules {
		if rule.Pattern == "install.sh" && rule.Handler == "install" {
			hasInstallRule = true
		}
		if rule.Pattern == "Brewfile" && rule.Handler == "homebrew" {
			hasBrewfileRule = true
		}
		if rule.Pattern == "*" && rule.Handler == "symlink" {
			hasCatchallRule = true
		}
	}
	assert.True(t, hasInstallRule, "Should have install.sh rule")
	assert.True(t, hasBrewfileRule, "Should have Brewfile rule")
	assert.True(t, hasCatchallRule, "Should have catchall symlink rule")

	// Test mappings - should have defaults from embedded config
	assert.Equal(t, "bin", cfg.Mappings.Path)
	assert.Equal(t, "install.sh", cfg.Mappings.Install)
	assert.Equal(t, []string{"aliases.sh", "profile.sh", "login.sh"}, cfg.Mappings.Shell)
	assert.Equal(t, "Brewfile", cfg.Mappings.Homebrew)
}

func TestGenerateRulesFromMapping(t *testing.T) {
	tests := []struct {
		name     string
		config   *config.Config
		expected int // expected number of rules
		validate func(t *testing.T, rules []config.Rule)
	}{
		{
			name: "empty_mappings",
			config: &config.Config{
				Mappings: config.Mappings{},
			},
			expected: 0,
		},
		{
			name: "path_mapping_only",
			config: &config.Config{
				Mappings: config.Mappings{
					Path: "bin",
				},
			},
			expected: 1,
			validate: func(t *testing.T, rules []config.Rule) {
				assert.Equal(t, "bin", rules[0].Pattern)
				assert.Equal(t, "path", rules[0].Handler)
			},
		},
		{
			name: "install_mapping_only",
			config: &config.Config{
				Mappings: config.Mappings{
					Install: "setup.sh",
				},
			},
			expected: 1,
			validate: func(t *testing.T, rules []config.Rule) {
				assert.Equal(t, "setup.sh", rules[0].Pattern)
				assert.Equal(t, "install", rules[0].Handler)
			},
		},
		{
			name: "shell_mappings_with_placement_detection",
			config: &config.Config{
				Mappings: config.Mappings{
					Shell: []string{"*.sh", "*aliases.sh", "login.sh"},
				},
			},
			expected: 3,
			validate: func(t *testing.T, rules []config.Rule) {
				// Check all are shell handlers
				// Verify all rules have shell handler
				for _, rule := range rules {
					assert.Equal(t, "shell", rule.Handler)
				}
			},
		},
		{
			name: "homebrew_mapping",
			config: &config.Config{
				Mappings: config.Mappings{
					Homebrew: "Brewfile",
				},
			},
			expected: 1,
			validate: func(t *testing.T, rules []config.Rule) {
				assert.Equal(t, "Brewfile", rules[0].Pattern)
				assert.Equal(t, "homebrew", rules[0].Handler)
			},
		},
		{
			name: "all_mappings_combined",
			config: &config.Config{
				Mappings: config.Mappings{
					Path:     "bin",
					Install:  "install.sh",
					Shell:    []string{"bashrc"},
					Homebrew: "Brewfile",
				},
			},
			expected: 4,
			validate: func(t *testing.T, rules []config.Rule) {
				assert.Equal(t, "bin", rules[0].Pattern)
				assert.Equal(t, "path", rules[0].Handler)

				assert.Equal(t, "install.sh", rules[1].Pattern)
				assert.Equal(t, "install", rules[1].Handler)

				assert.Equal(t, "bashrc", rules[2].Pattern)
				assert.Equal(t, "shell", rules[2].Handler)

				assert.Equal(t, "Brewfile", rules[3].Pattern)
				assert.Equal(t, "homebrew", rules[3].Handler)
			},
		},
		{
			name: "ignore_mapping_single_pattern",
			config: &config.Config{
				Mappings: config.Mappings{
					Ignore: []string{".env.local"},
				},
			},
			expected: 1,
			validate: func(t *testing.T, rules []config.Rule) {
				assert.Equal(t, ".env.local", rules[0].Pattern)
				assert.Equal(t, "ignore", rules[0].Handler)
			},
		},
		{
			name: "ignore_mapping_multiple_patterns",
			config: &config.Config{
				Mappings: config.Mappings{
					Ignore: []string{".env.local", "secrets.json", "private/*", "*.key"},
				},
			},
			expected: 4,
			validate: func(t *testing.T, rules []config.Rule) {
				expectedPatterns := []string{".env.local", "secrets.json", "private/*", "*.key"}
				for i, rule := range rules {
					assert.Equal(t, expectedPatterns[i], rule.Pattern)
					assert.Equal(t, "ignore", rule.Handler)
				}
			},
		},
		{
			name: "all_mappings_with_ignore",
			config: &config.Config{
				Mappings: config.Mappings{
					Path:     "bin",
					Install:  "install.sh",
					Shell:    []string{"bashrc"},
					Homebrew: "Brewfile",
					Ignore:   []string{".env", "*.secret"},
				},
			},
			expected: 6,
			validate: func(t *testing.T, rules []config.Rule) {
				// First 4 are the regular mappings
				assert.Equal(t, "bin", rules[0].Pattern)
				assert.Equal(t, "path", rules[0].Handler)

				assert.Equal(t, "install.sh", rules[1].Pattern)
				assert.Equal(t, "install", rules[1].Handler)

				assert.Equal(t, "bashrc", rules[2].Pattern)
				assert.Equal(t, "shell", rules[2].Handler)

				assert.Equal(t, "Brewfile", rules[3].Pattern)
				assert.Equal(t, "homebrew", rules[3].Handler)

				// Last 2 are the ignore patterns
				assert.Equal(t, ".env", rules[4].Pattern)
				assert.Equal(t, "ignore", rules[4].Handler)

				assert.Equal(t, "*.secret", rules[5].Pattern)
				assert.Equal(t, "ignore", rules[5].Handler)
			},
		},
		{
			name: "shell_placement_detection_complex",
			config: &config.Config{
				Mappings: config.Mappings{
					Shell: []string{
						"profile",
						"bash_profile",
						"bash_aliases",
						"zshrc",
						"bash_login",
						"random.sh",
					},
				},
			},
			expected: 6,
			validate: func(t *testing.T, rules []config.Rule) {
				// Just verify all rules have shell handler
				assert.Len(t, rules, 6)
				for _, rule := range rules {
					assert.Equal(t, "shell", rule.Handler)
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rules := tt.config.GenerateRulesFromMapping()
			assert.Len(t, rules, tt.expected)
			if tt.validate != nil {
				tt.validate(t, rules)
			}
		})
	}
}
