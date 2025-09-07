// Test Type: Business Logic Test
// Description: Tests pack-level ignore pattern integration with rule system

package rules_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestLoadPackRules_WithPackIgnore(t *testing.T) {
	// Create temp directory
	tempDir := t.TempDir()
	packPath := filepath.Join(tempDir, "mypack")
	err := os.Mkdir(packPath, 0755)
	require.NoError(t, err)

	// Create pack config with [pack] ignore
	configPath := filepath.Join(packPath, ".dodot.toml")
	configContent := `[pack]
ignore = ["*.log", "tmp/*", "cache"]

[mappings]
ignore = ["*.env"]
shell = ["*.sh"]
`
	err = os.WriteFile(configPath, []byte(configContent), 0644)
	require.NoError(t, err)

	// Load pack rules
	packRules, err := rules.LoadPackRules(packPath)
	require.NoError(t, err)

	// Verify ignore rules are generated and come first
	require.GreaterOrEqual(t, len(packRules), 3, "Should have at least 3 ignore rules")

	// Check pack ignore rules come first
	assert.Equal(t, "!*.log", packRules[0].Pattern)
	assert.Equal(t, "exclude", packRules[0].Handler)

	assert.Equal(t, "!tmp/*", packRules[1].Pattern)
	assert.Equal(t, "exclude", packRules[1].Handler)

	assert.Equal(t, "!cache", packRules[2].Pattern)
	assert.Equal(t, "exclude", packRules[2].Handler)

	// Check that mappings.ignore rules also exist (after pack.ignore)
	foundEnvIgnore := false
	for _, rule := range packRules {
		if rule.Pattern == "!*.env" {
			foundEnvIgnore = true
			break
		}
	}
	assert.True(t, foundEnvIgnore, "Should have *.env ignore rule from mappings")
}

func TestScanner_PackIgnoreIntegration(t *testing.T) {
	// Create test environment
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Create pack with files
	packFiles := map[string]string{
		"config.toml":   "app config",
		"install.sh":    "#!/bin/bash",
		"error.log":     "log content",
		"app.log":       "more logs",
		"tmp/cache.dat": "cache data",
		"tmp/temp.txt":  "temp file",
		"cache":         "cache file",
		"secrets.env":   "SECRET=value",
		".dodot.toml": `[pack]
ignore = ["*.log", "tmp/*", "cache"]

[mappings]
ignore = ["*.env"]
`,
	}

	// Write files to pack
	packPath := filepath.Join(env.DotfilesRoot, "mypack")
	for file, content := range packFiles {
		filePath := filepath.Join(packPath, file)
		dir := filepath.Dir(filePath)
		if dir != packPath {
			err := env.FS.MkdirAll(dir, 0755)
			require.NoError(t, err)
		}
		err := env.FS.WriteFile(filePath, []byte(content), 0644)
		require.NoError(t, err)
	}

	// Create pack
	pack := types.Pack{
		Name: "mypack",
		Path: packPath,
	}

	// Load rules
	globalRules := rules.GetDefaultRules()
	packRules, err := rules.LoadPackRulesFS(pack.Path, env.FS)
	require.NoError(t, err)

	// Merge rules
	effectiveRules := rules.MergeRules(globalRules, packRules)

	// Create scanner and scan
	scanner := rules.NewScannerWithFS(effectiveRules, env.FS)
	matches, err := scanner.ScanPack(pack)
	require.NoError(t, err)

	// Verify only non-ignored files are matched
	matchedFiles := make(map[string]bool)
	for _, match := range matches {
		matchedFiles[match.FilePath] = true
	}

	// These should be matched
	assert.True(t, matchedFiles["config.toml"], "config.toml should be matched")
	assert.True(t, matchedFiles["install.sh"], "install.sh should be matched")

	// These should be ignored by pack.ignore
	assert.False(t, matchedFiles["error.log"], "*.log should be ignored")
	assert.False(t, matchedFiles["app.log"], "*.log should be ignored")
	assert.False(t, matchedFiles["tmp/cache.dat"], "tmp/* should be ignored")
	assert.False(t, matchedFiles["tmp/temp.txt"], "tmp/* should be ignored")
	assert.False(t, matchedFiles["cache"], "cache should be ignored")

	// This should be ignored by mappings.ignore
	assert.False(t, matchedFiles["secrets.env"], "*.env should be ignored")

	// .dodot.toml is always skipped
	assert.False(t, matchedFiles[".dodot.toml"], ".dodot.toml should be skipped")
}

func TestMergeRules_PackIgnoreTakesPrecedence(t *testing.T) {
	// Global rules that would match everything
	globalRules := []config.Rule{
		{Pattern: "*", Handler: "symlink"},
	}

	// Pack rules with ignore patterns
	packRules := []config.Rule{
		{Pattern: "!*.log", Handler: "exclude"},
		{Pattern: "!tmp/*", Handler: "exclude"},
		{Pattern: "*.sh", Handler: "shell"},
	}

	// Merge rules
	merged := rules.MergeRules(globalRules, packRules)

	// Pack rules should come first
	require.Len(t, merged, 4)
	assert.Equal(t, "!*.log", merged[0].Pattern)
	assert.Equal(t, "!tmp/*", merged[1].Pattern)
	assert.Equal(t, "*.sh", merged[2].Pattern)
	assert.Equal(t, "*", merged[3].Pattern) // Global catchall comes last
}
