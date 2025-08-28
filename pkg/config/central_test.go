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
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["gitconfig"])

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

	// Test mappings - should be empty by default
	assert.Empty(t, cfg.Mappings.Path)
	assert.Empty(t, cfg.Mappings.Install)
	assert.Empty(t, cfg.Mappings.Shell)
	assert.Empty(t, cfg.Mappings.Homebrew)
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
				assert.Equal(t, "bin/", rules[0].Pattern)
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
				for _, rule := range rules {
					assert.Equal(t, "shell", rule.Handler)
				}

				// Check specific placements
				for _, rule := range rules {
					switch rule.Pattern {
					case "*aliases.sh":
						assert.Equal(t, "aliases", rule.Options["placement"])
					case "login.sh":
						assert.Equal(t, "login", rule.Options["placement"])
					default:
						assert.Equal(t, "environment", rule.Options["placement"])
					}
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
				assert.Equal(t, "bin/", rules[0].Pattern)
				assert.Equal(t, "path", rules[0].Handler)

				assert.Equal(t, "install.sh", rules[1].Pattern)
				assert.Equal(t, "install", rules[1].Handler)

				assert.Equal(t, "bashrc", rules[2].Pattern)
				assert.Equal(t, "shell", rules[2].Handler)
				assert.Equal(t, "environment", rules[2].Options["placement"])

				assert.Equal(t, "Brewfile", rules[3].Pattern)
				assert.Equal(t, "homebrew", rules[3].Handler)
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
				placements := map[string]string{
					"profile":      "environment",
					"bash_profile": "environment",
					"bash_aliases": "aliases",
					"zshrc":        "environment",
					"bash_login":   "login",
					"random.sh":    "environment", // default
				}

				for _, rule := range rules {
					expectedPlacement, ok := placements[rule.Pattern]
					assert.True(t, ok, "Unexpected pattern: %s", rule.Pattern)
					assert.Equal(t, expectedPlacement, rule.Options["placement"],
						"Pattern %s should have placement %s", rule.Pattern, expectedPlacement)
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
