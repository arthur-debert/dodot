package pipeline

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// PackRulesConfig is a helper type for generating test config files
type PackRulesConfig struct {
	Rules []config.Rule
}

func TestGetPatternsForHandler(t *testing.T) {
	tests := []struct {
		name             string
		handler          string
		packRules        []config.Rule
		expectedPatterns []string
	}{
		{
			name:    "shell handler patterns from default rules",
			handler: "shell",
			expectedPatterns: []string{
				"profile.sh",
				"login.sh",
				"*aliases.sh",
				"aliases.sh", // From mappings
			},
		},
		{
			name:    "install handler exact match",
			handler: "install",
			expectedPatterns: []string{
				"install.sh",
			},
		},
		{
			name:    "path handler directory patterns",
			handler: "path",
			expectedPatterns: []string{
				"bin/",
				".local/bin/",
			},
		},
		{
			name:    "homebrew handler single pattern",
			handler: "homebrew",
			expectedPatterns: []string{
				"Brewfile",
			},
		},
		{
			name:    "symlink catchall pattern",
			handler: "symlink",
			expectedPatterns: []string{
				"*",
			},
		},
		// Pack rules test commented out because LoadPackRules isn't implemented yet
		// {
		// 	name:    "pack rules override global",
		// 	handler: "shell",
		// 	packRules: []config.Rule{
		// 		{Pattern: "shell-init.sh", Handler: "shell"},
		// 		{Pattern: "*.zsh", Handler: "shell"},
		// 	},
		// 	expectedPatterns: []string{
		// 		"shell-init.sh",
		// 		"*.zsh",
		// 		"profile.sh",
		// 		"login.sh",
		// 		"*aliases.sh",
		// 		"aliases.sh", // From mappings
		// 	},
		// },
		{
			name:             "no patterns for handler",
			handler:          "nonexistent",
			expectedPatterns: []string{},
		},
		// Pack rules test commented out because LoadPackRules isn't implemented yet
		// {
		// 	name:    "excludes exclusion patterns",
		// 	handler: "shell",
		// 	packRules: []config.Rule{
		// 		{Pattern: "!*.bak", Handler: "shell"},
		// 		{Pattern: "config.sh", Handler: "shell"},
		// 	},
		// 	expectedPatterns: []string{
		// 		"config.sh",
		// 		"profile.sh",
		// 		"login.sh",
		// 		"*aliases.sh",
		// 		"aliases.sh", // From mappings
		// 	},
		// },
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create a temporary directory for the test
			tempDir := t.TempDir()
			packPath := filepath.Join(tempDir, "testpack")

			// Create filesystem
			fs := testutil.NewMemoryFS()
			require.NoError(t, fs.MkdirAll(packPath, 0755))

			// Create pack
			pack := types.Pack{
				Name: "testpack",
				Path: packPath,
			}

			// Create pack rules file if pack rules are provided
			if tt.packRules != nil {
				rulesConfig := PackRulesConfig{Rules: tt.packRules}
				rulesPath := filepath.Join(packPath, ".dodot.toml")
				content := generatePackRulesToml(rulesConfig)
				require.NoError(t, fs.WriteFile(rulesPath, []byte(content), 0644))
			}

			// Get patterns for handler
			patterns, err := GetPatternsForHandler(tt.handler, pack)
			require.NoError(t, err)

			// Check results
			assert.ElementsMatch(t, tt.expectedPatterns, patterns,
				"Expected patterns %v, got %v", tt.expectedPatterns, patterns)
		})
	}
}

func TestGetAllHandlerPatterns(t *testing.T) {
	// Create a temporary directory for the test
	tempDir := t.TempDir()
	packPath := filepath.Join(tempDir, "testpack")

	// Create filesystem
	fs := testutil.NewMemoryFS()
	require.NoError(t, fs.MkdirAll(packPath, 0755))

	// Create pack
	pack := types.Pack{
		Name: "testpack",
		Path: packPath,
	}

	// Get all handler patterns
	patterns, err := GetAllHandlerPatterns(pack)
	require.NoError(t, err)

	// Verify we get patterns for all handlers (except those with no patterns)
	expectedHandlers := []string{"shell", "install", "homebrew", "path", "symlink"}
	for _, handler := range expectedHandlers {
		assert.Contains(t, patterns, handler, "Missing patterns for handler %s", handler)
		assert.NotEmpty(t, patterns[handler], "Empty patterns for handler %s", handler)
	}

	// Verify specific patterns
	assert.Contains(t, patterns["shell"], "*aliases.sh")
	assert.Contains(t, patterns["install"], "install.sh")
	assert.Contains(t, patterns["homebrew"], "Brewfile")
	assert.Contains(t, patterns["path"], "bin/")
	assert.Contains(t, patterns["symlink"], "*")
}

func TestSuggestFilenameForHandler(t *testing.T) {
	tests := []struct {
		name             string
		handler          string
		patterns         []string
		expectedFilename string
	}{
		{
			name:             "exact match pattern",
			handler:          "install",
			patterns:         []string{"install.sh"},
			expectedFilename: "install.sh",
		},
		{
			name:             "glob pattern with prefix",
			handler:          "shell",
			patterns:         []string{"*aliases.sh"},
			expectedFilename: "aliases.sh",
		},
		{
			name:             "glob pattern with suffix",
			handler:          "shell",
			patterns:         []string{"profile*"},
			expectedFilename: "profile.sh",
		},
		{
			name:             "directory pattern skipped",
			handler:          "path",
			patterns:         []string{"bin/", ".local/bin/"},
			expectedFilename: "bin/",
		},
		{
			name:             "multiple patterns - prefer exact",
			handler:          "shell",
			patterns:         []string{"*aliases.sh", "profile.sh", "*.zsh"},
			expectedFilename: "profile.sh",
		},
		{
			name:             "no patterns",
			handler:          "custom",
			patterns:         []string{},
			expectedFilename: "",
		},
		{
			name:             "symlink catchall returns empty",
			handler:          "symlink",
			patterns:         []string{"*"},
			expectedFilename: "",
		},
		{
			name:             "handler default when only complex globs",
			handler:          "shell",
			patterns:         []string{"**/*.sh", "[a-z]*.sh"},
			expectedFilename: "shell.sh",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filename := SuggestFilenameForHandler(tt.handler, tt.patterns)
			assert.Equal(t, tt.expectedFilename, filename)
		})
	}
}

func TestGetHandlersNeedingFiles(t *testing.T) {
	tests := []struct {
		name             string
		packFiles        map[string]string
		expectedHandlers []string
	}{
		{
			name:             "empty pack needs all handlers",
			packFiles:        map[string]string{},
			expectedHandlers: []string{"shell", "homebrew", "install", "path"},
		},
		{
			name: "pack with shell file",
			packFiles: map[string]string{
				"aliases.sh": "# aliases",
			},
			expectedHandlers: []string{"homebrew", "install", "path"},
		},
		{
			name: "pack with multiple handler files",
			packFiles: map[string]string{
				"profile.sh": "# profile",
				"install.sh": "#!/bin/bash",
				"Brewfile":   "brew 'git'",
			},
			expectedHandlers: []string{"path"},
		},
		{
			name: "pack with all handlers",
			packFiles: map[string]string{
				"aliases.sh": "# aliases",
				"install.sh": "#!/bin/bash",
				"Brewfile":   "brew 'git'",
				"bin/test":   "#!/bin/bash",
			},
			expectedHandlers: []string{},
		},
		{
			name: "symlink-only files don't count",
			packFiles: map[string]string{
				"vimrc":     "\" vim config",
				"gitconfig": "[user]",
			},
			expectedHandlers: []string{"shell", "homebrew", "install", "path"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create a temporary directory for the test
			tempDir := t.TempDir()
			packPath := filepath.Join(tempDir, "testpack")

			// Create filesystem
			fs := testutil.NewMemoryFS()
			require.NoError(t, fs.MkdirAll(packPath, 0755))

			// Create pack files
			for filename, content := range tt.packFiles {
				fullPath := filepath.Join(packPath, filename)
				// Create directory if needed
				if dir := filepath.Dir(fullPath); dir != packPath {
					require.NoError(t, fs.MkdirAll(dir, 0755))
				}
				require.NoError(t, fs.WriteFile(fullPath, []byte(content), 0644))
			}

			// Create pack
			pack := types.Pack{
				Name: "testpack",
				Path: packPath,
			}

			// Get handlers needing files
			handlers, err := GetHandlersNeedingFiles(pack, fs)
			require.NoError(t, err)

			// Check results
			assert.ElementsMatch(t, tt.expectedHandlers, handlers,
				"Expected handlers %v, got %v", tt.expectedHandlers, handlers)
		})
	}
}

// Helper function to generate pack rules TOML content
func generatePackRulesToml(config PackRulesConfig) string {
	content := ""
	for _, rule := range config.Rules {
		content += "[[rules]]\n"
		content += `pattern = "` + rule.Pattern + `"` + "\n"
		content += `handler = "` + rule.Handler + `"` + "\n"
		if len(rule.Options) > 0 {
			content += "[rules.options]\n"
			for k, v := range rule.Options {
				content += k + ` = "` + v.(string) + `"` + "\n"
			}
		}
		content += "\n"
	}
	return content
}
