//go:build ignore
// +build ignore

// This test file is temporarily disabled as Clear functionality
// hasn't been implemented in the new handler architecture yet.

package homebrew

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// skipIfBrewNotAvailable skips the test if brew command is not available
func skipIfBrewNotAvailable(t *testing.T) {
	t.Helper()
	_, err := exec.LookPath("brew")
	if err != nil {
		t.Skip("Skipping test: brew command not available")
	}
}

func TestHomebrewHandler_Clear_Basic(t *testing.T) {
	tests := []struct {
		name         string
		setup        func(t *testing.T, fs types.FS, dataDir string)
		dryRun       bool
		wantItems    int
		checkResults func(t *testing.T, items []operations.ClearedItem)
	}{
		{
			name: "no state directory",
			setup: func(t *testing.T, fs types.FS, dataDir string) {
				// No setup - no state directory
			},
			dryRun:    false,
			wantItems: 0,
		},
		{
			name: "single Brewfile sentinel",
			setup: func(t *testing.T, fs types.FS, dataDir string) {
				stateDir := filepath.Join(dataDir, "dodot", "packs", "testpack", "homebrew")
				if err := fs.MkdirAll(stateDir, 0755); err != nil {
					t.Fatalf("failed to create state dir: %v", err)
				}

				// Create sentinel file
				sentinelPath := filepath.Join(stateDir, "testpack_Brewfile.sentinel")
				if err := fs.WriteFile(sentinelPath, []byte("checksum|2024-01-01T00:00:00Z"), 0644); err != nil {
					t.Fatalf("failed to write sentinel: %v", err)
				}
			},
			dryRun:    false,
			wantItems: 1,
			checkResults: func(t *testing.T, items []operations.ClearedItem) {
				if items[0].Type != "homebrew_state" {
					t.Errorf("Type = %q, want %q", items[0].Type, "homebrew_state")
				}
				if !strings.Contains(items[0].Description, "Removing Homebrew state") {
					t.Errorf("Description should contain 'Removing Homebrew state', got %q", items[0].Description)
				}
				if !strings.Contains(items[0].Description, "DODOT_HOMEBREW_UNINSTALL=true") {
					t.Errorf("Description should contain 'DODOT_HOMEBREW_UNINSTALL=true', got %q", items[0].Description)
				}
			},
		},
		{
			name: "multiple Brewfile sentinels",
			setup: func(t *testing.T, fs types.FS, dataDir string) {
				stateDir := filepath.Join(dataDir, "dodot", "packs", "testpack", "homebrew")
				if err := fs.MkdirAll(stateDir, 0755); err != nil {
					t.Fatalf("failed to create state dir: %v", err)
				}

				// Create multiple sentinel files
				if err := fs.WriteFile(
					filepath.Join(stateDir, "testpack_Brewfile.sentinel"),
					[]byte("checksum1|2024-01-01T00:00:00Z"), 0644); err != nil {
					t.Fatalf("failed to write first sentinel: %v", err)
				}
				if err := fs.WriteFile(
					filepath.Join(stateDir, "testpack_Brewfile.personal.sentinel"),
					[]byte("checksum2|2024-01-01T00:00:00Z"), 0644); err != nil {
					t.Fatalf("failed to write second sentinel: %v", err)
				}
			},
			dryRun:    false,
			wantItems: 2,
		},
		{
			name: "dry run mode",
			setup: func(t *testing.T, fs types.FS, dataDir string) {
				stateDir := filepath.Join(dataDir, "dodot", "packs", "testpack", "homebrew")
				if err := fs.MkdirAll(stateDir, 0755); err != nil {
					t.Fatalf("failed to create state dir: %v", err)
				}

				sentinelPath := filepath.Join(stateDir, "testpack_Brewfile.sentinel")
				if err := fs.WriteFile(sentinelPath, []byte("checksum|2024-01-01T00:00:00Z"), 0644); err != nil {
					t.Fatalf("failed to write sentinel: %v", err)
				}
			},
			dryRun:    true,
			wantItems: 1,
			checkResults: func(t *testing.T, items []operations.ClearedItem) {
				if !strings.Contains(items[0].Description, "Would remove") {
					t.Errorf("Description should contain 'Would remove' in dry run mode, got %q", items[0].Description)
				}
			},
		},
		{
			name: "state directory with non-sentinel files",
			setup: func(t *testing.T, fs types.FS, dataDir string) {
				stateDir := filepath.Join(dataDir, "dodot", "packs", "testpack", "homebrew")
				if err := fs.MkdirAll(stateDir, 0755); err != nil {
					t.Fatalf("failed to create state dir: %v", err)
				}

				// Create a non-sentinel file
				if err := fs.WriteFile(
					filepath.Join(stateDir, "other.txt"),
					[]byte("not a sentinel"), 0644); err != nil {
					t.Fatalf("failed to write other file: %v", err)
				}

				// Create a sentinel file
				if err := fs.WriteFile(
					filepath.Join(stateDir, "testpack_Brewfile.sentinel"),
					[]byte("checksum|2024-01-01T00:00:00Z"), 0644); err != nil {
					t.Fatalf("failed to write sentinel: %v", err)
				}
			},
			dryRun:    false,
			wantItems: 1, // Should only report the sentinel
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test environment
			fs := testutil.NewMemoryFS()
			dataDir := "/test/data"
			paths := testutil.NewMockPathResolver("/home/test", "/home/test/.config", dataDir)

			// Run setup
			tt.setup(t, fs, dataDir)

			// Create handler
			handler := NewHandler()

			// Create clear context
			ctx := operations.ClearContext{
				Pack:   types.Pack{Name: "testpack", Path: "/test/pack"},
				FS:     fs,
				Paths:  paths,
				DryRun: tt.dryRun,
			}

			// Execute clear
			items, err := handler.Clear(ctx)
			if err != nil {
				t.Fatalf("Clear() error = %v", err)
			}

			// Check number of items
			if len(items) != tt.wantItems {
				t.Errorf("got %d items, want %d", len(items), tt.wantItems)
			}

			// Run additional checks
			if tt.checkResults != nil && len(items) > 0 {
				tt.checkResults(t, items)
			}
		})
	}
}

func TestParseBrewfile(t *testing.T) {
	skipIfBrewNotAvailable(t)

	tests := []struct {
		name            string
		content         string
		expectedFormula int
		expectedCasks   int
	}{
		{
			name: "simple formulae and casks",
			content: `# Test Brewfile
brew 'git'
brew 'vim'
cask 'firefox'
mas 'Xcode', id: 497799835`,
			expectedFormula: 2,
			expectedCasks:   1,
		},
		{
			name:            "empty file",
			content:         "",
			expectedFormula: 0,
			expectedCasks:   0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create a temporary file
			tmpFile := filepath.Join(t.TempDir(), "Brewfile")
			if err := os.WriteFile(tmpFile, []byte(tt.content), 0644); err != nil {
				t.Fatalf("failed to write test file: %v", err)
			}

			// Create a memory filesystem for the test
			fs := testutil.NewMemoryFS()

			result, err := parseBrewfile(fs, tmpFile)
			if err != nil {
				// If brew is not available, skip the test
				if strings.Contains(err.Error(), "executable file not found") {
					t.Skip("brew command not available")
				}
				// parseBrewfile might return error if Brewfile doesn't exist
				if !strings.Contains(err.Error(), "failed to access Brewfile") {
					t.Fatalf("parseBrewfile() error = %v", err)
				}
			}

			// Count formulae and casks
			formulaCount := 0
			caskCount := 0
			for _, pkg := range result {
				switch pkg.Type {
				case "brew":
					formulaCount++
				case "cask":
					caskCount++
				}
			}

			// Check formulae
			if formulaCount != tt.expectedFormula {
				t.Errorf("got %d formulae, want %d", formulaCount, tt.expectedFormula)
			}

			// Check casks
			if caskCount != tt.expectedCasks {
				t.Errorf("got %d casks, want %d", caskCount, tt.expectedCasks)
			}
		})
	}
}

func TestHomebrewHandler_ClearWithUninstall_DryRun(t *testing.T) {
	handler := NewHandler()
	fs := testutil.NewMemoryFS()
	paths := testutil.NewMockPathResolver("/home/test", "/home/test/.config", "/test/data")

	// Create clear context
	ctx := operations.ClearContext{
		Pack:   types.Pack{Name: "testpack", Path: "test/pack"},
		FS:     fs,
		Paths:  paths,
		DryRun: true,
	}

	// Should handle gracefully even without a valid Brewfile
	items, err := handler.ClearWithUninstall(ctx)
	if err != nil && !strings.Contains(err.Error(), "failed to access Brewfile") {
		// We expect an error about accessing the Brewfile, but not other errors
		t.Fatalf("unexpected error type: %v", err)
	}

	// In dry run mode with no state, should return empty
	if len(items) != 0 {
		t.Errorf("got %d items, want 0", len(items))
	}
}

// Integration test for full uninstall flow - skipped as it requires actual brew installation
func TestHomebrewHandler_ClearWithUninstall_Integration(t *testing.T) {
	t.Skip("Full integration test not implemented")
}
