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
				testutil.CreateFile(t, root, "README.txxt", "# Dotfiles")
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
[files]
"test.conf" = "symlink"
"*.bak" = "ignore"
`
				testutil.CreateFile(t, pack, ".dodot.toml", config)
				return []string{pack}
			},
			expectedCount: 1,
			validate: func(t *testing.T, packs []types.Pack) {
				pack := packs[0]
				testutil.AssertEqual(t, 2, len(pack.Config.Files))
				testutil.AssertEqual(t, "symlink", pack.Config.Files["test.conf"])
				testutil.AssertEqual(t, "ignore", pack.Config.Files["*.bak"])
			},
		},
		{
			name: "skip pack with .dodotignore",
			setup: func(t *testing.T) []string {
				root := testutil.TempDir(t, "dotfiles")

				// Enabled pack
				pack1 := testutil.CreateDir(t, root, "enabled-pack")

				// Ignored pack
				pack2 := testutil.CreateDir(t, root, "ignored-pack")
				testutil.CreateFile(t, pack2, ".dodotignore", "")

				return []string{pack1, pack2}
			},
			expectedCount: 1,
			validate: func(t *testing.T, packs []types.Pack) {
				testutil.AssertEqual(t, "enabled-pack", packs[0].Name)
			},
		},
		{
			name: "sort by name alphabetically",
			setup: func(t *testing.T) []string {
				root := testutil.TempDir(t, "dotfiles")

				pack1 := testutil.CreateDir(t, root, "zebra-pack")
				pack2 := testutil.CreateDir(t, root, "alpha-pack")
				pack3 := testutil.CreateDir(t, root, "beta-pack")
				pack4 := testutil.CreateDir(t, root, "default-pack")

				return []string{pack1, pack2, pack3, pack4}
			},
			expectedCount: 4,
			validate: func(t *testing.T, packs []types.Pack) {
				// Expected alphabetical order
				expectedOrder := []string{"alpha-pack", "beta-pack", "default-pack", "zebra-pack"}
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
[files]
"test.vim" = "symlink"
"*.bak" = "ignore"
"setup.sh" = "install"
`,
			validate: func(t *testing.T, config types.PackConfig) {
				testutil.AssertEqual(t, 3, len(config.Files))
				testutil.AssertEqual(t, "symlink", config.Files["test.vim"])
				testutil.AssertEqual(t, "ignore", config.Files["*.bak"])
				testutil.AssertEqual(t, "install", config.Files["setup.sh"])
			},
		},
		{
			name: "minimal config",
			toml: ``,
			validate: func(t *testing.T, config types.PackConfig) {
				testutil.AssertEqual(t, 0, len(config.Files))
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

func TestValidatePack(t *testing.T) {
	tests := []struct {
		name    string
		setup   func(t *testing.T) string
		wantErr bool
		errCode errors.ErrorCode
	}{
		{
			name: "valid pack directory",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "dotfiles")
				pack := testutil.CreateDir(t, root, "valid-pack")
				testutil.CreateFile(t, pack, "alias.sh", "alias ll='ls -la'")
				return pack
			},
			wantErr: false,
		},
		{
			name: "valid pack with config",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "dotfiles")
				pack := testutil.CreateDir(t, root, "configured-pack")
				testutil.CreateFile(t, pack, ".dodot.toml", "description = \"Test\"")
				testutil.CreateFile(t, pack, "file.txt", "content")
				return pack
			},
			wantErr: false,
		},
		{
			name: "empty directory",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "dotfiles")
				pack := testutil.CreateDir(t, root, "empty-pack")
				return pack
			},
			wantErr: true,
			errCode: errors.ErrPackEmpty,
		},
		{
			name: "non-existent directory",
			setup: func(t *testing.T) string {
				return "/non/existent/pack"
			},
			wantErr: true,
			errCode: errors.ErrNotFound,
		},
		{
			name: "file instead of directory",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "dotfiles")
				file := testutil.CreateFile(t, root, "file.txt", "content")
				return file
			},
			wantErr: true,
			errCode: errors.ErrPackInvalid,
		},
		{
			name: "invalid config",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "dotfiles")
				pack := testutil.CreateDir(t, root, "bad-config-pack")
				testutil.CreateFile(t, pack, ".dodot.toml", "invalid = [toml")
				return pack
			},
			wantErr: true,
			errCode: errors.ErrConfigLoad,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			packPath := tt.setup(t)

			err := ValidatePack(packPath)

			if tt.wantErr {
				testutil.AssertError(t, err)
				if tt.errCode != "" {
					testutil.AssertTrue(t, errors.IsErrorCode(err, tt.errCode),
						"expected error code %s, got %s", tt.errCode, errors.GetErrorCode(err))
				}
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}

func TestSelectPacks(t *testing.T) {
	// Create test packs
	createTestPacks := func() []types.Pack {
		return []types.Pack{
			{Name: "vim"},
			{Name: "shell"},
			{Name: "bin"},
			{Name: "config"},
		}
	}

	tests := []struct {
		name          string
		allPacks      []types.Pack
		selectedNames []string
		expectedNames []string
		wantErr       bool
		errCode       errors.ErrorCode
	}{
		{
			name:          "select all when no names provided",
			allPacks:      createTestPacks(),
			selectedNames: []string{},
			expectedNames: []string{"vim", "shell", "bin", "config"},
			wantErr:       false,
		},
		{
			name:          "select specific packs",
			allPacks:      createTestPacks(),
			selectedNames: []string{"vim", "bin"},
			expectedNames: []string{"bin", "vim"}, // Alphabetical order
			wantErr:       false,
		},
		{
			name:          "maintain alphabetical order",
			allPacks:      createTestPacks(),
			selectedNames: []string{"vim", "bin", "shell"},
			expectedNames: []string{"bin", "shell", "vim"}, // Should be sorted alphabetically
			wantErr:       false,
		},
		{
			name:          "error on non-existent pack",
			allPacks:      createTestPacks(),
			selectedNames: []string{"vim", "non-existent"},
			wantErr:       true,
			errCode:       errors.ErrPackNotFound,
		},
		{
			name:          "error on multiple non-existent packs",
			allPacks:      createTestPacks(),
			selectedNames: []string{"fake1", "vim", "fake2"},
			wantErr:       true,
			errCode:       errors.ErrPackNotFound,
		},
		{
			name:          "empty pack list",
			allPacks:      []types.Pack{},
			selectedNames: []string{"anything"},
			wantErr:       true,
			errCode:       errors.ErrPackNotFound,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			selected, err := SelectPacks(tt.allPacks, tt.selectedNames)

			if tt.wantErr {
				testutil.AssertError(t, err)
				if tt.errCode != "" {
					testutil.AssertTrue(t, errors.IsErrorCode(err, tt.errCode),
						"expected error code %s, got %s", tt.errCode, errors.GetErrorCode(err))
				}

				// Check error details
				if tt.errCode == errors.ErrPackNotFound {
					details := errors.GetErrorDetails(err)
					testutil.AssertNotNil(t, details["notFound"])
					testutil.AssertNotNil(t, details["available"])
				}
			} else {
				testutil.AssertNoError(t, err)
				testutil.AssertEqual(t, len(tt.expectedNames), len(selected))

				for i, name := range tt.expectedNames {
					testutil.AssertEqual(t, name, selected[i].Name,
						"pack at index %d", i)
				}
			}
		})
	}
}

func TestGetPackNames(t *testing.T) {
	packs := []types.Pack{
		{Name: "pack1"},
		{Name: "pack2"},
		{Name: "pack3"},
	}

	names := getPackNames(packs)
	expected := []string{"pack1", "pack2", "pack3"}

	testutil.AssertSliceEqual(t, expected, names)
}

func TestGetFileAction(t *testing.T) {
	tests := []struct {
		name     string
		config   types.PackConfig
		filename string
		expected string
	}{
		{
			name:     "empty config",
			config:   types.PackConfig{},
			filename: "test.txt",
			expected: "",
		},
		{
			name: "exact match",
			config: types.PackConfig{
				Files: map[string]string{
					"test.txt": "ignore",
					"app.sh":   "install",
				},
			},
			filename: "test.txt",
			expected: "ignore",
		},
		{
			name: "glob pattern match",
			config: types.PackConfig{
				Files: map[string]string{
					"*.bak":    "ignore",
					"*.sh":     "install",
					"init.vim": "symlink",
				},
			},
			filename: "backup.bak",
			expected: "ignore",
		},
		{
			name: "exact match takes precedence over pattern",
			config: types.PackConfig{
				Files: map[string]string{
					"*.txt":       "ignore",
					"special.txt": "symlink",
				},
			},
			filename: "special.txt",
			expected: "symlink",
		},
		{
			name: "no match",
			config: types.PackConfig{
				Files: map[string]string{
					"*.bak": "ignore",
				},
			},
			filename: "test.txt",
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.config.GetFileAction(tt.filename)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}
