package homebrew

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// mockClearPaths implements the Paths interface for testing
type mockClearPaths struct {
	dataDir string
}

func (m *mockClearPaths) PackHandlerDir(packName, handlerName string) string {
	return filepath.Join(m.dataDir, "packs", packName, handlerName)
}

func (m *mockClearPaths) MapPackFileToSystem(pack *types.Pack, relPath string) string {
	return filepath.Join("home", relPath)
}

func TestHomebrewHandler_Clear_Basic(t *testing.T) {
	tests := []struct {
		name         string
		setup        func(t *testing.T, fs types.FS, dataDir string, packPath string)
		dryRun       bool
		wantItems    int
		checkResults func(t *testing.T, items []types.ClearedItem)
	}{
		{
			name: "no state directory",
			setup: func(t *testing.T, fs types.FS, dataDir string, packPath string) {
				// No setup - no state directory
			},
			dryRun:    false,
			wantItems: 0,
		},
		{
			name: "single Brewfile sentinel",
			setup: func(t *testing.T, fs types.FS, dataDir string, packPath string) {
				stateDir := filepath.Join(dataDir, "packs", "testpack", "homebrew")
				require.NoError(t, fs.MkdirAll(stateDir, 0755))

				// Create sentinel file
				sentinelPath := filepath.Join(stateDir, "testpack_Brewfile.sentinel")
				require.NoError(t, fs.WriteFile(sentinelPath, []byte("checksum|2024-01-01T00:00:00Z"), 0644))
			},
			dryRun:    false,
			wantItems: 1,
			checkResults: func(t *testing.T, items []types.ClearedItem) {
				assert.Equal(t, "homebrew_state", items[0].Type)
				assert.Contains(t, items[0].Description, "Removing Homebrew state")
				assert.Contains(t, items[0].Description, "DODOT_HOMEBREW_UNINSTALL=true")
			},
		},
		{
			name: "multiple Brewfile sentinels",
			setup: func(t *testing.T, fs types.FS, dataDir string, packPath string) {
				stateDir := filepath.Join(dataDir, "packs", "testpack", "homebrew")
				require.NoError(t, fs.MkdirAll(stateDir, 0755))

				// Create multiple sentinel files
				require.NoError(t, fs.WriteFile(
					filepath.Join(stateDir, "testpack_Brewfile.sentinel"),
					[]byte("checksum1|2024-01-01T00:00:00Z"), 0644))
				require.NoError(t, fs.WriteFile(
					filepath.Join(stateDir, "testpack_Brewfile.personal.sentinel"),
					[]byte("checksum2|2024-01-01T00:00:00Z"), 0644))
			},
			dryRun:    false,
			wantItems: 2,
		},
		{
			name: "dry run mode",
			setup: func(t *testing.T, fs types.FS, dataDir string, packPath string) {
				stateDir := filepath.Join(dataDir, "packs", "testpack", "homebrew")
				require.NoError(t, fs.MkdirAll(stateDir, 0755))

				sentinelPath := filepath.Join(stateDir, "testpack_Brewfile.sentinel")
				require.NoError(t, fs.WriteFile(sentinelPath, []byte("checksum|2024-01-01T00:00:00Z"), 0644))
			},
			dryRun:    true,
			wantItems: 1,
			checkResults: func(t *testing.T, items []types.ClearedItem) {
				assert.Contains(t, items[0].Description, "Would remove")
			},
		},
		{
			name: "state directory with non-sentinel files",
			setup: func(t *testing.T, fs types.FS, dataDir string, packPath string) {
				stateDir := filepath.Join(dataDir, "packs", "testpack", "homebrew")
				require.NoError(t, fs.MkdirAll(stateDir, 0755))

				// Create a non-sentinel file
				require.NoError(t, fs.WriteFile(
					filepath.Join(stateDir, "some-other-file"),
					[]byte("data"), 0644))
			},
			dryRun:    false,
			wantItems: 1,
			checkResults: func(t *testing.T, items []types.ClearedItem) {
				assert.Equal(t, "homebrew_state", items[0].Type)
				assert.Contains(t, items[0].Description, "Removing Homebrew state directory")
			},
		},
	}

	// Make sure DODOT_HOMEBREW_UNINSTALL is not set
	_ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL")

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test environment
			dataDir := "test/data"
			packPath := "test/pack"

			fs := testutil.NewTestFS()
			require.NoError(t, fs.MkdirAll(packPath, 0755))

			// Run setup
			if tt.setup != nil {
				tt.setup(t, fs, dataDir, packPath)
			}

			// Create handler and context
			h := NewHomebrewHandler()
			ctx := types.ClearContext{
				Pack: types.Pack{
					Name: "testpack",
					Path: packPath,
				},
				DataStore: nil, // Not used in basic clear
				FS:        fs,
				Paths:     &mockClearPaths{dataDir: dataDir},
				DryRun:    tt.dryRun,
			}

			// Execute clear
			items, err := h.Clear(ctx)
			require.NoError(t, err)

			// Check results
			assert.Len(t, items, tt.wantItems)
			if tt.checkResults != nil {
				tt.checkResults(t, items)
			}
		})
	}
}

func TestParseBrewfile(t *testing.T) {
	tests := []struct {
		name     string
		content  string
		expected []brewPackage
	}{
		{
			name: "simple formulae and casks",
			content: `# Comment
brew "git"
brew "vim"
cask "visual-studio-code"
`,
			expected: []brewPackage{
				{Name: "git", Type: "brew", Brewfile: "Brewfile"},
				{Name: "vim", Type: "brew", Brewfile: "Brewfile"},
				{Name: "visual-studio-code", Type: "cask", Brewfile: "Brewfile"},
			},
		},
		{
			name: "quoted package names",
			content: `brew "git-lfs"
brew 'nodejs'
cask "google-chrome"
cask 'firefox'`,
			expected: []brewPackage{
				{Name: "firefox", Type: "cask", Brewfile: "Brewfile"}, // Sorted alphabetically
				{Name: "git-lfs", Type: "brew", Brewfile: "Brewfile"},
				{Name: "google-chrome", Type: "cask", Brewfile: "Brewfile"},
				{Name: "nodejs", Type: "brew", Brewfile: "Brewfile"},
			},
		},
		{
			name: "packages with options",
			content: `brew "mysql", restart_service: true
brew "postgresql@14"
cask "docker", args: { appdir: "/Applications" }`,
			expected: []brewPackage{
				{Name: "docker", Type: "cask", Brewfile: "Brewfile"}, // Sorted alphabetically
				{Name: "mysql", Type: "brew", Brewfile: "Brewfile"},
				{Name: "postgresql@14", Type: "brew", Brewfile: "Brewfile"},
			},
		},
		{
			name: "empty file",
			content: `# Just comments

# Nothing here`,
			expected: []brewPackage{},
		},
		{
			name: "mixed formatting",
			content: `   brew   "wget"   
cask	"slack"
brew "curl"
  # Comment in between
  brew "htop"`,
			expected: []brewPackage{
				{Name: "curl", Type: "brew", Brewfile: "Brewfile"}, // Sorted alphabetically
				{Name: "htop", Type: "brew", Brewfile: "Brewfile"},
				{Name: "slack", Type: "cask", Brewfile: "Brewfile"},
				{Name: "wget", Type: "brew", Brewfile: "Brewfile"},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create a memory FS with the Brewfile
			fs := testutil.NewTestFS()
			brewfilePath := "test/Brewfile"
			require.NoError(t, fs.WriteFile(brewfilePath, []byte(tt.content), 0644))

			// Parse the Brewfile
			packages, err := parseBrewfile(fs, brewfilePath)
			require.NoError(t, err)

			// Check results (packages are sorted by name)
			assert.Equal(t, len(tt.expected), len(packages))
			for i, expected := range tt.expected {
				assert.Equal(t, expected.Name, packages[i].Name, "Package name mismatch at index %d", i)
				assert.Equal(t, expected.Type, packages[i].Type, "Package type mismatch at index %d", i)
				assert.Equal(t, expected.Brewfile, packages[i].Brewfile, "Brewfile mismatch at index %d", i)
			}
		})
	}
}

func TestExtractPackageName(t *testing.T) {
	tests := []struct {
		line     string
		prefix   string
		expected string
	}{
		// Basic cases
		{`brew "git"`, "brew", "git"},
		{`cask "firefox"`, "cask", "firefox"},
		{`brew 'vim'`, "brew", "vim"},

		// With options
		{`brew "mysql", restart_service: true`, "brew", "mysql"},
		{`cask "docker", args: { appdir: "/Applications" }`, "cask", "docker"},

		// Unquoted
		{`brew git`, "brew", "git"},
		{`cask firefox`, "cask", "firefox"},

		// Special characters
		{`brew "git-lfs"`, "brew", "git-lfs"},
		{`brew "postgresql@14"`, "brew", "postgresql@14"},

		// Edge cases
		{`brew`, "brew", ""},
		{`brew ""`, "brew", ""},
		{`brew ''`, "brew", ""},
	}

	for _, tt := range tests {
		t.Run(fmt.Sprintf("%s/%s", tt.prefix, tt.line), func(t *testing.T) {
			result := extractPackageName(tt.line, tt.prefix)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestHomebrewHandler_ClearWithUninstall_DryRun(t *testing.T) {
	// This test verifies the dry run behavior without actually calling brew

	// Create test environment
	dataDir := "test/data"
	packPath := "test/pack"

	fs := testutil.NewTestFS()
	require.NoError(t, fs.MkdirAll(packPath, 0755))

	// Create Brewfile
	brewfileContent := `brew "git"
brew "vim"
cask "firefox"`
	require.NoError(t, fs.WriteFile(filepath.Join(packPath, "Brewfile"), []byte(brewfileContent), 0644))

	// Create state
	stateDir := filepath.Join(dataDir, "packs", "testpack", "homebrew")
	require.NoError(t, fs.MkdirAll(stateDir, 0755))
	require.NoError(t, fs.WriteFile(
		filepath.Join(stateDir, "testpack_Brewfile.sentinel"),
		[]byte("checksum|2024-01-01T00:00:00Z"), 0644))

	// Create handler and context
	h := NewHomebrewHandler()
	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: packPath,
		},
		DataStore: nil,
		FS:        fs,
		Paths:     &mockClearPaths{dataDir: dataDir},
		DryRun:    true, // Dry run mode
	}

	// Execute clear with uninstall
	items, err := h.ClearWithUninstall(ctx)
	require.NoError(t, err)

	// In dry run, we should see state removal but no actual uninstalls
	// (since we can't determine what's installed without calling brew)
	assert.GreaterOrEqual(t, len(items), 1)

	// Check for state removal item
	var hasStateRemoval bool
	for _, item := range items {
		if item.Type == "homebrew_state" {
			hasStateRemoval = true
			assert.Contains(t, item.Description, "Would remove")
		}
	}
	assert.True(t, hasStateRemoval, "Should have state removal item")
}

// Integration test that would require brew to be installed
func TestHomebrewHandler_ClearWithUninstall_Integration(t *testing.T) {
	if testing.Short() {
		t.Skip("Skipping integration test in short mode")
	}

	// Check if brew is available
	if _, err := os.Stat("/usr/local/bin/brew"); err != nil {
		if _, err := os.Stat("/opt/homebrew/bin/brew"); err != nil {
			t.Skip("Homebrew not installed, skipping integration test")
		}
	}

	// This would be a full integration test with actual brew commands
	// For now, we'll skip it as it would require:
	// 1. Installing test packages
	// 2. Running the clear operation
	// 3. Verifying packages were uninstalled
	// 4. Cleaning up

	t.Skip("Full integration test not implemented")
}
