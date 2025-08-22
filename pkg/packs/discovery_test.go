package packs

import (
	"fmt"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestShouldIgnoreWithPatterns(t *testing.T) {
	tests := []struct {
		name        string
		fileName    string
		patterns    []string
		wantIgnored bool
	}{
		{
			name:        "exact match",
			fileName:    ".git",
			patterns:    []string{".git"},
			wantIgnored: true,
		},
		{
			name:        "no match",
			fileName:    "mypack",
			patterns:    []string{".git"},
			wantIgnored: false,
		},
		{
			name:        "glob pattern match",
			fileName:    "test.tmp",
			patterns:    []string{"*.tmp"},
			wantIgnored: true,
		},
		{
			name:        "glob pattern no match",
			fileName:    "test.txt",
			patterns:    []string{"*.tmp"},
			wantIgnored: false,
		},
		{
			name:        "multiple patterns - match first",
			fileName:    ".git",
			patterns:    []string{".git", "*.tmp", "node_modules"},
			wantIgnored: true,
		},
		{
			name:        "multiple patterns - match last",
			fileName:    "node_modules",
			patterns:    []string{".git", "*.tmp", "node_modules"},
			wantIgnored: true,
		},
		{
			name:        "multiple patterns - no match",
			fileName:    "mypack",
			patterns:    []string{".git", "*.tmp", "node_modules"},
			wantIgnored: false,
		},
		{
			name:        "empty patterns",
			fileName:    ".git",
			patterns:    []string{},
			wantIgnored: false,
		},
		{
			name:        "complex glob pattern",
			fileName:    "backup.2023.tar",
			patterns:    []string{"backup.*.tar"},
			wantIgnored: true,
		},
		{
			name:        "case sensitive",
			fileName:    ".GIT",
			patterns:    []string{".git"},
			wantIgnored: false,
		},
		{
			name:        "question mark glob",
			fileName:    "test1.tmp",
			patterns:    []string{"test?.tmp"},
			wantIgnored: true,
		},
		{
			name:        "character class glob",
			fileName:    "test5.tmp",
			patterns:    []string{"test[0-9].tmp"},
			wantIgnored: true,
		},
		{
			name:        "character class no match",
			fileName:    "testA.tmp",
			patterns:    []string{"test[0-9].tmp"},
			wantIgnored: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := shouldIgnoreWithPatterns(tt.fileName, tt.patterns)
			assert.Equal(t, tt.wantIgnored, got, "shouldIgnoreWithPatterns(%q, %v)", tt.fileName, tt.patterns)
		})
	}
}

func TestShouldIgnoreWithPatterns_EdgeCases(t *testing.T) {
	tests := []struct {
		name        string
		fileName    string
		patterns    []string
		wantIgnored bool
		description string
	}{
		{
			name:        "empty filename",
			fileName:    "",
			patterns:    []string{"*"},
			wantIgnored: true,
			description: "empty filename should match wildcard",
		},
		{
			name:        "dot file",
			fileName:    ".hidden",
			patterns:    []string{".*"},
			wantIgnored: true,
			description: "dot files should match .* pattern",
		},
		{
			name:        "invalid pattern",
			fileName:    "test",
			patterns:    []string{"["},
			wantIgnored: false,
			description: "invalid pattern should not match (filepath.Match returns error)",
		},
		{
			name:        "path separator in name - match",
			fileName:    "dir/file",
			patterns:    []string{"dir/*"},
			wantIgnored: true,
			description: "on Unix, filepath.Match allows * to match path separators",
		},
		{
			name:        "path separator in pattern - match",
			fileName:    "dir/file",
			patterns:    []string{"dir/file"},
			wantIgnored: true,
			description: "exact match with path separator works",
		},
		{
			name:        "nil patterns",
			fileName:    "test",
			patterns:    nil,
			wantIgnored: false,
			description: "nil patterns should not match anything",
		},
		{
			name:        "empty pattern string",
			fileName:    "test",
			patterns:    []string{""},
			wantIgnored: false,
			description: "empty pattern should not match non-empty filename",
		},
		{
			name:        "empty pattern matches empty filename",
			fileName:    "",
			patterns:    []string{""},
			wantIgnored: true,
			description: "empty pattern should match empty filename",
		},
		{
			name:        "special characters in filename",
			fileName:    "test[1].tmp",
			patterns:    []string{"test[[]1].tmp"},
			wantIgnored: true,
			description: "escaped brackets should match literal brackets",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := shouldIgnoreWithPatterns(tt.fileName, tt.patterns)
			assert.Equal(t, tt.wantIgnored, got, tt.description)
		})
	}
}

func TestLoadPackConfigFS(t *testing.T) {
	tests := []struct {
		name        string
		configPath  string
		fileContent string
		setupFS     func(types.FS)
		wantConfig  types.PackConfig
		wantErr     bool
		errContains string
	}{
		{
			name:       "valid config with ignore rules",
			configPath: "test/pack/.dodot.toml",
			fileContent: `[[ignore]]
path = "*.tmp"

[[ignore]]
path = ".cache"`,
			wantConfig: types.PackConfig{
				Ignore: []types.IgnoreRule{
					{Path: "*.tmp"},
					{Path: ".cache"},
				},
			},
			wantErr: false,
		},
		{
			name:       "valid config with override rules",
			configPath: "test/pack/.dodot.toml",
			fileContent: `[[override]]
path = "special.sh"
handler = "provision"

[override.with]
priority = "high"`,
			wantConfig: types.PackConfig{
				Override: []types.OverrideRule{
					{
						Path:    "special.sh",
						Handler: "provision",
						With: map[string]interface{}{
							"priority": "high",
						},
					},
				},
			},
			wantErr: false,
		},
		{
			name:        "empty config file",
			configPath:  "test/pack/.dodot.toml",
			fileContent: " ",
			wantConfig:  types.PackConfig{},
			wantErr:     false,
		},
		{
			name:       "config with both ignore and override",
			configPath: "test/pack/.dodot.toml",
			fileContent: `[[ignore]]
path = "*.log"

[[override]]
path = "install.sh"
handler = "shell_profile"`,
			wantConfig: types.PackConfig{
				Ignore: []types.IgnoreRule{
					{Path: "*.log"},
				},
				Override: []types.OverrideRule{
					{
						Path:    "install.sh",
						Handler: "shell_profile",
					},
				},
			},
			wantErr: false,
		},
		{
			name:       "file not found",
			configPath: "test/pack/.dodot.toml",
			setupFS: func(fs types.FS) {
				// Don't add any files
			},
			wantErr:     true,
			errContains: "file does not exist",
		},
		{
			name:        "invalid TOML syntax",
			configPath:  "test/pack/.dodot.toml",
			fileContent: `[invalid toml syntax`,
			wantErr:     true,
			errContains: "failed to parse TOML",
		},
		{
			name:        "invalid field in TOML",
			configPath:  "test/pack/.dodot.toml",
			fileContent: `unknown_field = "value"`,
			wantConfig:  types.PackConfig{},
			wantErr:     false, // TOML will ignore unknown fields
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			fs := testutil.NewTestFS()

			if tt.setupFS != nil {
				tt.setupFS(fs)
			} else if tt.fileContent != "" {
				testutil.CreateFileT(t, fs, tt.configPath, tt.fileContent)
			}

			got, err := loadPackConfigFS(tt.configPath, fs)

			if tt.wantErr {
				require.Error(t, err)
				if tt.errContains != "" {
					assert.Contains(t, err.Error(), tt.errContains)
				}
			} else {
				require.NoError(t, err)
				assert.Equal(t, tt.wantConfig, got)
			}
		})
	}
}

func TestLoadPackConfigFS_EdgeCases(t *testing.T) {
	tests := []struct {
		name        string
		configPath  string
		fileContent string
		description string
		wantErr     bool
	}{
		{
			name:        "very large config file",
			configPath:  "test/.dodot.toml",
			fileContent: generateLargeConfig(100),
			description: "should handle large configs gracefully",
			wantErr:     false,
		},
		{
			name:       "config with comments",
			configPath: "test/.dodot.toml",
			fileContent: `# This is a comment
[[ignore]]
path = "*.tmp" # Ignore temp files

# Another comment
[[override]]
path = "test.sh"
handler = "symlink"`,
			description: "should parse configs with comments correctly",
			wantErr:     false,
		},
		{
			name:       "windows-style paths",
			configPath: `C:\test\pack\.dodot.toml`,
			fileContent: `[[ignore]]
path = "*.tmp"`,
			description: "should handle windows-style paths",
			wantErr:     false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			fs := testutil.NewTestFS()
			testutil.CreateFileT(t, fs, tt.configPath, tt.fileContent)

			_, err := loadPackConfigFS(tt.configPath, fs)

			if tt.wantErr {
				assert.Error(t, err, tt.description)
			} else {
				assert.NoError(t, err, tt.description)
			}
		})
	}
}

// Helper function to generate large config for testing
func generateLargeConfig(numRules int) string {
	config := ""
	for i := 0; i < numRules; i++ {
		config += fmt.Sprintf(`[[ignore]]
path = "pattern%d.tmp"

`, i)
	}
	return config
}
