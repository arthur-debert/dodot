package config

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestGenerateMatchersFromMapping(t *testing.T) {
	tests := []struct {
		name        string
		fileMapping FileMapping
		expected    int // expected number of matchers
		validate    func(t *testing.T, matchers []MatcherConfig)
	}{
		{
			name:        "empty file mapping",
			fileMapping: FileMapping{},
			expected:    0,
		},
		{
			name: "path mapping only",
			fileMapping: FileMapping{
				Path: "bin",
			},
			expected: 1,
			validate: func(t *testing.T, matchers []MatcherConfig) {
				assert.Equal(t, "mapped-path", matchers[0].Name)
				assert.Equal(t, "directory", matchers[0].Trigger.Type)
				assert.Equal(t, "bin", matchers[0].Trigger.Data["pattern"])
				assert.Equal(t, "path", matchers[0].Handler.Type)
				assert.Equal(t, 90, matchers[0].Priority)
			},
		},
		{
			name: "install mapping only",
			fileMapping: FileMapping{
				Install: "setup.sh",
			},
			expected: 1,
			validate: func(t *testing.T, matchers []MatcherConfig) {
				assert.Equal(t, "mapped-install", matchers[0].Name)
				assert.Equal(t, "filename", matchers[0].Trigger.Type)
				assert.Equal(t, "setup.sh", matchers[0].Trigger.Data["pattern"])
				assert.Equal(t, "install", matchers[0].Handler.Type)
				assert.Equal(t, 90, matchers[0].Priority)
			},
		},
		{
			name: "shell mappings with placement detection",
			fileMapping: FileMapping{
				Shell: []string{"aliases.sh", "profile.sh", "login.sh", "custom.sh"},
			},
			expected: 4,
			validate: func(t *testing.T, matchers []MatcherConfig) {
				// Check aliases placement
				assert.Equal(t, "mapped-shell-0", matchers[0].Name)
				assert.Equal(t, "aliases.sh", matchers[0].Trigger.Data["pattern"])
				assert.Equal(t, "aliases", matchers[0].Handler.Data["placement"])

				// Check profile placement (defaults to environment)
				assert.Equal(t, "mapped-shell-1", matchers[1].Name)
				assert.Equal(t, "profile.sh", matchers[1].Trigger.Data["pattern"])
				assert.Equal(t, "environment", matchers[1].Handler.Data["placement"])

				// Check login placement
				assert.Equal(t, "mapped-shell-2", matchers[2].Name)
				assert.Equal(t, "login.sh", matchers[2].Trigger.Data["pattern"])
				assert.Equal(t, "login", matchers[2].Handler.Data["placement"])

				// Check custom placement (defaults to environment)
				assert.Equal(t, "mapped-shell-3", matchers[3].Name)
				assert.Equal(t, "custom.sh", matchers[3].Trigger.Data["pattern"])
				assert.Equal(t, "environment", matchers[3].Handler.Data["placement"])

				// All should have shell handler and priority 80
				for _, m := range matchers {
					assert.Equal(t, "filename", m.Trigger.Type)
					assert.Equal(t, "shell", m.Handler.Type)
					assert.Equal(t, 80, m.Priority)
				}
			},
		},
		{
			name: "homebrew mapping only",
			fileMapping: FileMapping{
				Homebrew: "Brewfile.local",
			},
			expected: 1,
			validate: func(t *testing.T, matchers []MatcherConfig) {
				assert.Equal(t, "mapped-homebrew", matchers[0].Name)
				assert.Equal(t, "filename", matchers[0].Trigger.Type)
				assert.Equal(t, "Brewfile.local", matchers[0].Trigger.Data["pattern"])
				assert.Equal(t, "homebrew", matchers[0].Handler.Type)
				assert.Equal(t, 90, matchers[0].Priority)
			},
		},
		{
			name: "complete file mapping",
			fileMapping: FileMapping{
				Path:     "scripts",
				Install:  "install.sh",
				Shell:    []string{"env.sh"},
				Homebrew: "Brewfile",
			},
			expected: 4,
			validate: func(t *testing.T, matchers []MatcherConfig) {
				// Should have one of each type
				var foundPath, foundInstall, foundShell, foundHomebrew bool
				for _, m := range matchers {
					switch m.Name {
					case "mapped-path":
						foundPath = true
						assert.Equal(t, "scripts", m.Trigger.Data["pattern"])
					case "mapped-install":
						foundInstall = true
						assert.Equal(t, "install.sh", m.Trigger.Data["pattern"])
					case "mapped-shell-0":
						foundShell = true
						assert.Equal(t, "env.sh", m.Trigger.Data["pattern"])
					case "mapped-homebrew":
						foundHomebrew = true
						assert.Equal(t, "Brewfile", m.Trigger.Data["pattern"])
					}
				}
				assert.True(t, foundPath, "path matcher not found")
				assert.True(t, foundInstall, "install matcher not found")
				assert.True(t, foundShell, "shell matcher not found")
				assert.True(t, foundHomebrew, "homebrew matcher not found")
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := &Config{
				FileMapping: tt.fileMapping,
			}
			matchers := cfg.GenerateMatchersFromMapping()
			assert.Len(t, matchers, tt.expected)
			if tt.validate != nil {
				tt.validate(t, matchers)
			}
		})
	}
}

func TestPostProcessConfig(t *testing.T) {
	t.Run("adds catchall exclude from special files", func(t *testing.T) {
		cfg := &Config{
			Patterns: Patterns{
				SpecialFiles: SpecialFiles{
					PackConfig: ".dodot.toml",
					IgnoreFile: ".dodotignore",
				},
			},
		}

		err := postProcessConfig(cfg)
		assert.NoError(t, err)
		assert.Equal(t, []string{".dodot.toml", ".dodotignore"}, cfg.Patterns.CatchallExclude)
	})

	t.Run("combines default and file_mapping matchers", func(t *testing.T) {
		cfg := &Config{
			FileMapping: FileMapping{
				Path:    "custom-bin",
				Install: "custom-install.sh",
			},
			Patterns: Patterns{
				SpecialFiles: SpecialFiles{
					PackConfig: ".dodot.toml",
					IgnoreFile: ".dodotignore",
				},
			},
		}

		err := postProcessConfig(cfg)
		assert.NoError(t, err)

		// Should have default matchers + 2 from file_mapping
		assert.Greater(t, len(cfg.Matchers), 2)

		// Check that file_mapping matchers were added
		var foundCustomPath, foundCustomInstall bool
		for _, m := range cfg.Matchers {
			if m.Name == "mapped-path" && m.Trigger.Data["pattern"] == "custom-bin" {
				foundCustomPath = true
			}
			if m.Name == "mapped-install" && m.Trigger.Data["pattern"] == "custom-install.sh" {
				foundCustomInstall = true
			}
		}
		assert.True(t, foundCustomPath, "custom path matcher not found")
		assert.True(t, foundCustomInstall, "custom install matcher not found")

		// Check that default matchers are still there
		var foundDefaultInstall bool
		for _, m := range cfg.Matchers {
			if m.Name == "install-script" && m.Trigger.Data["pattern"] == "install.sh" {
				foundDefaultInstall = true
			}
		}
		assert.True(t, foundDefaultInstall, "default install matcher not found")
	})

	t.Run("preserves existing matchers and adds file_mapping", func(t *testing.T) {
		cfg := &Config{
			Matchers: []MatcherConfig{
				{
					Name:     "custom-matcher",
					Priority: 100,
					Trigger:  TriggerConfig{Type: "filename", Data: map[string]interface{}{"pattern": "custom.txt"}},
					Handler:  HandlerConfig{Type: "symlink"},
				},
			},
			FileMapping: FileMapping{
				Path: "bin",
			},
			Patterns: Patterns{
				SpecialFiles: SpecialFiles{
					PackConfig: ".dodot.toml",
					IgnoreFile: ".dodotignore",
				},
			},
		}

		err := postProcessConfig(cfg)
		assert.NoError(t, err)

		// Should have the custom matcher + file_mapping matcher
		assert.GreaterOrEqual(t, len(cfg.Matchers), 2)

		// Check custom matcher is preserved
		assert.Equal(t, "custom-matcher", cfg.Matchers[0].Name)

		// Check file_mapping matcher was added
		found := false
		for _, m := range cfg.Matchers {
			if m.Name == "mapped-path" {
				found = true
				break
			}
		}
		assert.True(t, found, "mapped-path matcher not found")
	})
}
