// Test Type: Unit Test
// Description: Tests for the rules package - scanner that matches files against rules

package rules_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestScanner_ScanPack(t *testing.T) {
	t.Run("exact_filename_match", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		// Setup files
		packConfig := testutil.PackConfig{
			Files: map[string]string{
				"install.sh": "#!/bin/bash",
				"README.md":  "# README",
			},
		}
		testPack := env.SetupPack("testpack", packConfig)

		// Create scanner with rules
		ruleList := []config.Rule{
			{Pattern: "install.sh", Handler: "install"},
			{Pattern: "*", Handler: "symlink"},
		}
		scanner := rules.NewScannerWithFS(ruleList, env.FS)

		// Create pack struct
		pack := types.Pack{
			Name: testPack.Name,
			Path: testPack.Path,
		}

		// Scan
		matches, err := scanner.ScanPack(pack)
		require.NoError(t, err)

		// Verify matches
		assert.Len(t, matches, 2)

		matchMap := make(map[string]string)
		for _, m := range matches {
			matchMap[m.FileName] = m.Handler
		}

		assert.Equal(t, "install", matchMap["install.sh"])
		assert.Equal(t, "symlink", matchMap["README.md"])
	})

	t.Run("glob_pattern_match", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		packConfig := testutil.PackConfig{
			Files: map[string]string{
				"aliases.sh":    "alias ll='ls -la'",
				"my-aliases.sh": "alias g='git'",
				"config":        "some config",
			},
		}
		testPack := env.SetupPack("testpack", packConfig)

		ruleList := []config.Rule{
			{Pattern: "*aliases.sh", Handler: "shell"},
			{Pattern: "*", Handler: "symlink"},
		}
		scanner := rules.NewScannerWithFS(ruleList, env.FS)

		pack := types.Pack{
			Name: testPack.Name,
			Path: testPack.Path,
		}

		matches, err := scanner.ScanPack(pack)
		require.NoError(t, err)
		assert.Len(t, matches, 3)

		matchMap := make(map[string]string)
		for _, m := range matches {
			matchMap[m.FileName] = m.Handler
		}

		assert.Equal(t, "shell", matchMap["aliases.sh"])
		assert.Equal(t, "shell", matchMap["my-aliases.sh"])
		assert.Equal(t, "symlink", matchMap["config"])
	})

	t.Run("directory_pattern", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		packConfig := testutil.PackConfig{
			Files: map[string]string{
				"bin.txt": "not a directory",
			},
		}
		testPack := env.SetupPack("testpack", packConfig)

		// Manually create directories
		testPack.AddDirectory("bin")
		testPack.AddDirectory("lib")

		ruleList := []config.Rule{
			{Pattern: "bin/", Handler: "path"},
			{Pattern: "*", Handler: "symlink"},
		}
		scanner := rules.NewScannerWithFS(ruleList, env.FS)

		pack := types.Pack{
			Name: testPack.Name,
			Path: testPack.Path,
		}

		matches, err := scanner.ScanPack(pack)
		require.NoError(t, err)

		matchMap := make(map[string]string)
		for _, m := range matches {
			matchMap[m.FileName] = m.Handler
		}

		// Only bin and bin.txt match (lib doesn't match * for directories)
		assert.Len(t, matches, 2)
		assert.Equal(t, "path", matchMap["bin"])
		assert.NotContains(t, matchMap, "lib") // Directories don't match *
		assert.Equal(t, "symlink", matchMap["bin.txt"])
	})

	t.Run("exclusion_patterns", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		packConfig := testutil.PackConfig{
			Files: map[string]string{
				"config":     "main config",
				"config.bak": "backup",
				".DS_Store":  "mac file",
			},
		}
		testPack := env.SetupPack("testpack", packConfig)

		ruleList := []config.Rule{
			{Pattern: "!*.bak"},
			{Pattern: "!.DS_Store"},
			{Pattern: "*", Handler: "symlink"},
		}
		scanner := rules.NewScannerWithFS(ruleList, env.FS)

		pack := types.Pack{
			Name: testPack.Name,
			Path: testPack.Path,
		}

		matches, err := scanner.ScanPack(pack)
		require.NoError(t, err)

		matchMap := make(map[string]bool)
		for _, m := range matches {
			matchMap[m.FileName] = true
		}

		// Only config should match (config.bak excluded by rule, .DS_Store skipped by scanner)
		assert.Len(t, matches, 1)
		assert.True(t, matchMap["config"])
		assert.False(t, matchMap["config.bak"])
		assert.False(t, matchMap[".DS_Store"]) // Not even seen by rules
	})

	t.Run("rule_precedence", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		packConfig := testutil.PackConfig{
			Files: map[string]string{
				"test.sh": "#!/bin/bash",
			},
		}
		testPack := env.SetupPack("testpack", packConfig)

		ruleList := []config.Rule{
			{Pattern: "test.sh", Handler: "install"}, // Exact match
			{Pattern: "*.sh", Handler: "shell"},      // Glob pattern
			{Pattern: "*", Handler: "symlink"},       // Catchall
		}
		scanner := rules.NewScannerWithFS(ruleList, env.FS)

		pack := types.Pack{
			Name: testPack.Name,
			Path: testPack.Path,
		}

		matches, err := scanner.ScanPack(pack)
		require.NoError(t, err)
		assert.Len(t, matches, 1)
		assert.Equal(t, "install", matches[0].Handler)
	})

	t.Run("top_level_only", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		packConfig := testutil.PackConfig{
			Files: map[string]string{
				"setup.sh":       "#!/bin/bash",
				"config/app.yml": "app config", // This creates config dir but scanner won't see app.yml
			},
		}
		testPack := env.SetupPack("testpack", packConfig)

		ruleList := []config.Rule{
			{Pattern: "*.sh", Handler: "shell"},
			{Pattern: "*", Handler: "symlink"},
		}
		scanner := rules.NewScannerWithFS(ruleList, env.FS)

		pack := types.Pack{
			Name: testPack.Name,
			Path: testPack.Path,
		}

		matches, err := scanner.ScanPack(pack)
		require.NoError(t, err)

		// Scanner only reads top-level entries
		fileHandlers := make(map[string]string)
		for _, m := range matches {
			fileHandlers[m.FileName] = m.Handler
		}

		assert.Equal(t, "shell", fileHandlers["setup.sh"])
		assert.NotContains(t, fileHandlers, "config")  // Directory doesn't match *
		assert.NotContains(t, fileHandlers, "app.yml") // Nested file not seen
	})

	t.Run("options_passed_through", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		packConfig := testutil.PackConfig{
			Files: map[string]string{
				"profile.sh": "export PATH",
			},
		}
		testPack := env.SetupPack("testpack", packConfig)

		ruleList := []config.Rule{
			{
				Pattern: "profile.sh",
				Handler: "shell",
				Options: map[string]interface{}{
					"placement": "environment",
				},
			},
		}
		scanner := rules.NewScannerWithFS(ruleList, env.FS)

		pack := types.Pack{
			Name: testPack.Name,
			Path: testPack.Path,
		}

		matches, err := scanner.ScanPack(pack)
		require.NoError(t, err)
		assert.Len(t, matches, 1)
		assert.Equal(t, "environment", matches[0].Options["placement"])
	})
}

func TestScanner_HiddenFiles(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	packConfig := testutil.PackConfig{
		Files: map[string]string{
			"normal.txt":           "normal file",
			".hidden":              "hidden file",
			".config/settings.yml": "settings",
			".gitignore":           "*.log",
		},
	}
	testPack := env.SetupPack("testpack", packConfig)

	scanner := rules.NewScannerWithFS([]config.Rule{
		{Pattern: "*", Handler: "symlink"},
	}, env.FS)

	pack := types.Pack{
		Name: testPack.Name,
		Path: testPack.Path,
	}

	matches, err := scanner.ScanPack(pack)
	require.NoError(t, err)

	// Count matches by filename
	matchedFiles := make(map[string]bool)
	for _, m := range matches {
		matchedFiles[m.FileName] = true
	}

	// Scanner only reads top-level and skips .gitignore
	// Directories don't match * pattern
	assert.True(t, matchedFiles["normal.txt"])
	assert.True(t, matchedFiles[".hidden"])
	assert.False(t, matchedFiles[".config"])      // Directory doesn't match *
	assert.False(t, matchedFiles["settings.yml"]) // Nested file not seen
	assert.False(t, matchedFiles[".gitignore"])   // Skipped by scanner
}

func TestScanner_EmptyPack(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Create empty pack
	testPack := env.SetupPack("empty", testutil.PackConfig{})

	scanner := rules.NewScannerWithFS([]config.Rule{
		{Pattern: "*", Handler: "symlink"},
	}, env.FS)

	pack := types.Pack{
		Name: testPack.Name,
		Path: testPack.Path,
	}

	matches, err := scanner.ScanPack(pack)
	assert.NoError(t, err)
	assert.Empty(t, matches)
}

func TestScanner_ComplexExclusions(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	packConfig := testutil.PackConfig{
		Files: map[string]string{
			"config":       "main config",
			"config~":      "backup 1",
			"config.bak":   "backup 2",
			"config.swp":   "swap file",
			"#config#":     "emacs backup",
			".DS_Store":    "mac file",
			"important.sh": "#!/bin/bash",
		},
	}
	testPack := env.SetupPack("testpack", packConfig)

	// Use default-like exclusion rules
	scanner := rules.NewScannerWithFS([]config.Rule{
		{Pattern: "!*.bak"},
		{Pattern: "!*.swp"},
		{Pattern: "!*~"},
		{Pattern: "!#*#"},
		{Pattern: "!.DS_Store"},
		{Pattern: "*", Handler: "symlink"},
	}, env.FS)

	pack := types.Pack{
		Name: testPack.Name,
		Path: testPack.Path,
	}

	matches, err := scanner.ScanPack(pack)
	require.NoError(t, err)

	matchedFiles := make(map[string]bool)
	for _, m := range matches {
		matchedFiles[m.FileName] = true
	}

	// .DS_Store is skipped by scanner, others excluded by rules
	assert.Len(t, matches, 2)
	assert.True(t, matchedFiles["config"])
	assert.True(t, matchedFiles["important.sh"])
	assert.False(t, matchedFiles["config~"])
	assert.False(t, matchedFiles["config.bak"])
	assert.False(t, matchedFiles["config.swp"])
	assert.False(t, matchedFiles["#config#"])
	assert.False(t, matchedFiles[".DS_Store"]) // Skipped by scanner, not rules
}

func TestNewScanner(t *testing.T) {
	// Test that NewScanner creates a scanner without filesystem
	ruleList := []config.Rule{
		{Pattern: "*", Handler: "symlink"},
	}

	scanner := rules.NewScanner(ruleList)
	assert.NotNil(t, scanner)
}

func TestMatch_Fields(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	packConfig := testutil.PackConfig{
		Files: map[string]string{
			"file.txt": "content", // Top level file
		},
	}
	testPack := env.SetupPack("mypack", packConfig)

	scanner := rules.NewScannerWithFS([]config.Rule{
		{Pattern: "*", Handler: "symlink", Options: map[string]interface{}{"key": "value"}},
	}, env.FS)

	pack := types.Pack{
		Name: testPack.Name,
		Path: testPack.Path,
	}

	matches, err := scanner.ScanPack(pack)
	require.NoError(t, err)

	require.Len(t, matches, 1)
	fileMatch := matches[0]

	assert.Equal(t, "mypack", fileMatch.PackName)
	assert.Equal(t, "file.txt", fileMatch.FilePath)
	assert.Equal(t, "file.txt", fileMatch.FileName)
	assert.False(t, fileMatch.IsDirectory)
	assert.Equal(t, "symlink", fileMatch.Handler)
	assert.Equal(t, "value", fileMatch.Options["key"])
}
