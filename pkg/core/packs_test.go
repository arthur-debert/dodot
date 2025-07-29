package core

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func init() {
	// Set logging to error level for tests to reduce noise
	logging.SetupLogger(0)
}

func TestGetPackCandidates(t *testing.T) {
	tests := []struct {
		name          string
		setup         func(t *testing.T) string
		expectedCount int
		expectedNames []string
		wantErr       bool
		errCode       errors.ErrorCode
	}{
		{
			name: "valid dotfiles directory",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "dotfiles")
				testutil.CreateDir(t, root, "vim-pack")
				testutil.CreateDir(t, root, "shell-pack")
				testutil.CreateDir(t, root, "bin-pack")
				testutil.CreateFile(t, root, "README.md", "# Dotfiles")
				return root
			},
			expectedCount: 3,
			expectedNames: []string{"bin-pack", "shell-pack", "vim-pack"},
			wantErr:       false,
		},
		{
			name: "ignores hidden directories except .config",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "dotfiles")
				testutil.CreateDir(t, root, "normal-pack")
				testutil.CreateDir(t, root, ".git")
				testutil.CreateDir(t, root, ".hidden")
				testutil.CreateDir(t, root, ".config")
				return root
			},
			expectedCount: 2,
			expectedNames: []string{".config", "normal-pack"},
			wantErr:       false,
		},
		{
			name: "ignores default patterns",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "dotfiles")
				testutil.CreateDir(t, root, "good-pack")
				testutil.CreateDir(t, root, "node_modules")
				testutil.CreateDir(t, root, ".svn")
				testutil.CreateFile(t, root, ".DS_Store", "")
				return root
			},
			expectedCount: 1,
			expectedNames: []string{"good-pack"},
			wantErr:       false,
		},
		{
			name: "empty directory",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "empty")
			},
			expectedCount: 0,
			expectedNames: []string{},
			wantErr:       false,
		},
		{
			name: "non-existent directory",
			setup: func(t *testing.T) string {
				return "/non/existent/path"
			},
			wantErr: true,
			errCode: errors.ErrNotFound,
		},
		{
			name: "file instead of directory",
			setup: func(t *testing.T) string {
				dir := testutil.TempDir(t, "test")
				file := testutil.CreateFile(t, dir, "file.txt", "content")
				return file
			},
			wantErr: true,
			errCode: errors.ErrInvalidInput,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			root := tt.setup(t)
			
			candidates, err := GetPackCandidates(root)
			
			if tt.wantErr {
				testutil.AssertError(t, err)
				if tt.errCode != "" {
					testutil.AssertTrue(t, errors.IsErrorCode(err, tt.errCode),
						"expected error code %s, got %s", tt.errCode, errors.GetErrorCode(err))
				}
				return
			}
			
			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tt.expectedCount, len(candidates),
				"unexpected number of candidates")
			
			// Extract just the base names for comparison
			var names []string
			for _, c := range candidates {
				names = append(names, filepath.Base(c))
			}
			
			if len(tt.expectedNames) > 0 {
				testutil.AssertSliceEqual(t, tt.expectedNames, names)
			}
		})
	}
}

func TestGetPacks(t *testing.T) {
	tests := []struct {
		name          string
		setup         func(t *testing.T) []string
		expectedCount int
		validate      func(t *testing.T, packs []types.Pack)
	}{
		{
			name: "load simple packs",
			setup: func(t *testing.T) []string {
				root := testutil.TempDir(t, "dotfiles")
				pack1 := testutil.CreateDir(t, root, "pack1")
				pack2 := testutil.CreateDir(t, root, "pack2")
				return []string{pack1, pack2}
			},
			expectedCount: 2,
			validate: func(t *testing.T, packs []types.Pack) {
				testutil.AssertEqual(t, "pack1", packs[0].Name)
				testutil.AssertEqual(t, "pack2", packs[1].Name)
			},
		},
		{
			name: "load pack with config",
			setup: func(t *testing.T) []string {
				root := testutil.TempDir(t, "dotfiles")
				pack := testutil.CreateDir(t, root, "configured-pack")
				
				config := `
description = "A configured pack"
priority = 10

[[matchers]]
trigger = "filename"
pattern = "*.conf"
powerup = "symlink"
`
				testutil.CreateFile(t, pack, ".dodot.toml", config)
				return []string{pack}
			},
			expectedCount: 1,
			validate: func(t *testing.T, packs []types.Pack) {
				pack := packs[0]
				testutil.AssertEqual(t, "A configured pack", pack.Description)
				testutil.AssertEqual(t, 10, pack.Priority)
				testutil.AssertEqual(t, 1, len(pack.Config.Matchers))
			},
		},
		{
			name: "skip disabled pack",
			setup: func(t *testing.T) []string {
				root := testutil.TempDir(t, "dotfiles")
				
				// Enabled pack
				pack1 := testutil.CreateDir(t, root, "enabled-pack")
				
				// Disabled pack
				pack2 := testutil.CreateDir(t, root, "disabled-pack")
				testutil.CreateFile(t, pack2, ".dodot.toml", "disabled = true")
				
				return []string{pack1, pack2}
			},
			expectedCount: 1,
			validate: func(t *testing.T, packs []types.Pack) {
				testutil.AssertEqual(t, "enabled-pack", packs[0].Name)
			},
		},
		{
			name: "sort by priority and name",
			setup: func(t *testing.T) []string {
				root := testutil.TempDir(t, "dotfiles")
				
				// Low priority pack
				pack1 := testutil.CreateDir(t, root, "zebra-pack")
				testutil.CreateFile(t, pack1, ".dodot.toml", "priority = 1")
				
				// High priority pack
				pack2 := testutil.CreateDir(t, root, "alpha-pack")
				testutil.CreateFile(t, pack2, ".dodot.toml", "priority = 10")
				
				// Same high priority pack (should sort by name)
				pack3 := testutil.CreateDir(t, root, "beta-pack")
				testutil.CreateFile(t, pack3, ".dodot.toml", "priority = 10")
				
				// Default priority (0)
				pack4 := testutil.CreateDir(t, root, "default-pack")
				
				return []string{pack1, pack2, pack3, pack4}
			},
			expectedCount: 4,
			validate: func(t *testing.T, packs []types.Pack) {
				// Expected order: alpha (10), beta (10), zebra (1), default (0)
				expectedOrder := []string{"alpha-pack", "beta-pack", "zebra-pack", "default-pack"}
				for i, name := range expectedOrder {
					testutil.AssertEqual(t, name, packs[i].Name,
						"pack at index %d", i)
				}
			},
		},
		{
			name: "invalid pack config",
			setup: func(t *testing.T) []string {
				root := testutil.TempDir(t, "dotfiles")
				
				// Valid pack
				pack1 := testutil.CreateDir(t, root, "good-pack")
				
				// Pack with invalid TOML
				pack2 := testutil.CreateDir(t, root, "bad-pack")
				testutil.CreateFile(t, pack2, ".dodot.toml", "invalid = [toml")
				
				return []string{pack1, pack2}
			},
			expectedCount: 1, // Should skip the bad pack
			validate: func(t *testing.T, packs []types.Pack) {
				testutil.AssertEqual(t, "good-pack", packs[0].Name)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			candidates := tt.setup(t)
			
			packs, err := GetPacks(candidates)
			
			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tt.expectedCount, len(packs),
				"unexpected number of packs")
			
			if tt.validate != nil {
				tt.validate(t, packs)
			}
		})
	}
}

func TestLoadPackConfig(t *testing.T) {
	tests := []struct {
		name     string
		toml     string
		validate func(t *testing.T, config types.PackConfig)
		wantErr  bool
	}{
		{
			name: "complete config",
			toml: `
description = "Test pack"
priority = 5
disabled = false

[[matchers]]
name = "vim-files"
trigger = "filename"
powerup = "symlink"
pattern = ".*vim*"
target = "$HOME"
priority = 10

[[matchers]]
trigger = "directory"
powerup = "bin"
pattern = "bin"

[powerup_options]
[powerup_options.symlink]
force = true
backup = true
`,
			validate: func(t *testing.T, config types.PackConfig) {
				testutil.AssertEqual(t, "Test pack", config.Description)
				testutil.AssertEqual(t, 5, config.Priority)
				testutil.AssertFalse(t, config.Disabled)
				testutil.AssertEqual(t, 2, len(config.Matchers))
				
				// First matcher
				m1 := config.Matchers[0]
				testutil.AssertEqual(t, "vim-files", m1.Name)
				testutil.AssertEqual(t, "filename", m1.Trigger)
				testutil.AssertEqual(t, "symlink", m1.PowerUp)
				testutil.AssertEqual(t, ".*vim*", m1.Pattern)
				testutil.AssertEqual(t, "$HOME", m1.Target)
				testutil.AssertEqual(t, 10, m1.Priority)
				
				// Second matcher
				m2 := config.Matchers[1]
				testutil.AssertEqual(t, "directory", m2.Trigger)
				testutil.AssertEqual(t, "bin", m2.PowerUp)
				
				// PowerUp options
				symlinkOpts := config.PowerUpOptions["symlink"]
				testutil.AssertNotNil(t, symlinkOpts)
				testutil.AssertEqual(t, true, symlinkOpts["force"].(bool))
				testutil.AssertEqual(t, true, symlinkOpts["backup"].(bool))
			},
		},
		{
			name: "minimal config",
			toml: `description = "Minimal"`,
			validate: func(t *testing.T, config types.PackConfig) {
				testutil.AssertEqual(t, "Minimal", config.Description)
				testutil.AssertEqual(t, 0, config.Priority)
				testutil.AssertFalse(t, config.Disabled)
				testutil.AssertEqual(t, 0, len(config.Matchers))
			},
		},
		{
			name: "matcher with options",
			toml: `
[[matchers]]
trigger = "filename"
powerup = "symlink"

[matchers.options]
caseSensitive = false
recursive = true

[matchers.trigger_options]
pattern = "*.conf"

[matchers.powerup_options]
target = "$HOME/.config"
`,
			validate: func(t *testing.T, config types.PackConfig) {
				testutil.AssertEqual(t, 1, len(config.Matchers))
				m := config.Matchers[0]
				
				// Check that options are parsed (even if not used in current impl)
				testutil.AssertNotNil(t, m.Options)
				testutil.AssertNotNil(t, m.TriggerOptions)
				testutil.AssertNotNil(t, m.PowerUpOptions)
			},
		},
		{
			name:    "invalid toml",
			toml:    `invalid = [toml`,
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create temporary config file
			dir := testutil.TempDir(t, "config-test")
			configPath := testutil.CreateFile(t, dir, ".dodot.toml", tt.toml)
			
			config, err := loadPackConfig(configPath)
			
			if tt.wantErr {
				testutil.AssertError(t, err)
				return
			}
			
			testutil.AssertNoError(t, err)
			if tt.validate != nil {
				tt.validate(t, config)
			}
		})
	}
}

func TestShouldIgnore(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected bool
	}{
		{"git directory", ".git", true},
		{"svn directory", ".svn", true},
		{"node_modules", "node_modules", true},
		{"DS_Store", ".DS_Store", true},
		{"swap file", "file.swp", true},
		{"backup file", "file~", true},
		{"emacs backup", "#file#", true},
		{"normal directory", "my-pack", false},
		{"config directory", ".config", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := shouldIgnore(tt.input)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

func TestFileExists(t *testing.T) {
	dir := testutil.TempDir(t, "exists-test")
	existingFile := testutil.CreateFile(t, dir, "exists.txt", "content")
	nonExisting := filepath.Join(dir, "not-exists.txt")
	
	testutil.AssertTrue(t, config.FileExists(existingFile))
	testutil.AssertFalse(t, config.FileExists(nonExisting))
}

// Integration test
func TestPackDiscoveryIntegration(t *testing.T) {
	// Create a realistic dotfiles structure
	root := testutil.CreateDotfilesRepo(t)
	
	// Get candidates
	candidates, err := GetPackCandidates(root)
	testutil.AssertNoError(t, err)
	
	// Should find our test packs
	testutil.AssertTrue(t, len(candidates) >= 4,
		"expected at least 4 packs, got %d", len(candidates))
	
	// Load packs
	packs, err := GetPacks(candidates)
	testutil.AssertNoError(t, err)
	
	// Verify we got the expected packs
	packNames := make(map[string]bool)
	for _, p := range packs {
		packNames[p.Name] = true
	}
	
	expectedPacks := []string{"vim-pack", "shell-pack", "bin-pack", "config-pack"}
	for _, expected := range expectedPacks {
		testutil.AssertTrue(t, packNames[expected],
			"expected pack %s not found", expected)
	}
}

// Benchmark pack loading
func BenchmarkGetPacks(b *testing.B) {
	// Create test structure
	root := b.TempDir()
	var candidates []string
	
	// Create 50 packs
	for i := 0; i < 50; i++ {
		packName := filepath.Join(root, fmt.Sprintf("pack-%02d", i))
		if err := os.MkdirAll(packName, 0755); err != nil {
			b.Fatal(err)
		}
		candidates = append(candidates, packName)
		
		// Half with configs
		if i%2 == 0 {
			config := fmt.Sprintf(`description = "Pack %d"\npriority = %d`, i, i)
			configPath := filepath.Join(packName, ".dodot.toml")
			if err := os.WriteFile(configPath, []byte(config), 0644); err != nil {
				b.Fatal(err)
			}
		}
	}
	
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := GetPacks(candidates)
		if err != nil {
			b.Fatal(err)
		}
	}
}