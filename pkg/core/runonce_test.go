package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestShouldRunOnceAction(t *testing.T) {
	// Setup test environment
	tmpDir := testutil.TempDir(t, "runonce-test")
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	_ = os.Setenv("DODOT_DATA_DIR", tmpDir)
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	// Create sentinel directories
	homebrewDir := filepath.Join(tmpDir, "homebrew")
	installDir := filepath.Join(tmpDir, "install")
	_ = os.MkdirAll(homebrewDir, 0755)
	_ = os.MkdirAll(installDir, 0755)

	tests := []struct {
		name        string
		action      types.Action
		force       bool
		setupFunc   func()
		shouldRun   bool
		expectError bool
	}{
		{
			name: "non_runonce_action_always_runs",
			action: types.Action{
				Type: types.ActionTypeLink,
			},
			shouldRun: true,
		},
		{
			name: "force_flag_always_runs",
			action: types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"checksum": "abc123",
					"pack":     "tools",
				},
			},
			force:     true,
			shouldRun: true,
		},
		{
			name: "missing_checksum_runs",
			action: types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"pack": "tools",
				},
			},
			shouldRun: true,
		},
		{
			name: "missing_pack_runs",
			action: types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"checksum": "abc123",
				},
			},
			shouldRun: true,
		},
		{
			name: "no_sentinel_file_runs",
			action: types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"checksum": "abc123",
					"pack":     "tools",
				},
			},
			shouldRun: true,
		},
		{
			name: "matching_checksum_skips",
			action: types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"checksum": "abc123",
					"pack":     "tools",
				},
			},
			setupFunc: func() {
				sentinelPath := filepath.Join(homebrewDir, "tools")
				_ = os.WriteFile(sentinelPath, []byte("abc123"), 0644)
			},
			shouldRun: false,
		},
		{
			name: "different_checksum_runs",
			action: types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"checksum": "def456",
					"pack":     "tools",
				},
			},
			setupFunc: func() {
				sentinelPath := filepath.Join(homebrewDir, "tools")
				_ = os.WriteFile(sentinelPath, []byte("abc123"), 0644)
			},
			shouldRun: true,
		},
		{
			name: "install_action_no_sentinel",
			action: types.Action{
				Type: types.ActionTypeInstall,
				Metadata: map[string]interface{}{
					"checksum": "xyz789",
					"pack":     "dev",
				},
			},
			shouldRun: true,
		},
		{
			name: "install_action_matching_checksum",
			action: types.Action{
				Type: types.ActionTypeInstall,
				Metadata: map[string]interface{}{
					"checksum": "xyz789",
					"pack":     "dev",
				},
			},
			setupFunc: func() {
				sentinelPath := filepath.Join(installDir, "dev")
				_ = os.WriteFile(sentinelPath, []byte("xyz789"), 0644)
			},
			shouldRun: false,
		},
		{
			name: "sentinel_is_directory_runs",
			action: types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"checksum": "abc123",
					"pack":     "badsentinel",
				},
			},
			setupFunc: func() {
				sentinelPath := filepath.Join(homebrewDir, "badsentinel")
				_ = os.Mkdir(sentinelPath, 0755)
			},
			shouldRun: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Clean up any previous sentinel files
			_ = os.RemoveAll(homebrewDir)
			_ = os.RemoveAll(installDir)
			_ = os.MkdirAll(homebrewDir, 0755)
			_ = os.MkdirAll(installDir, 0755)

			if tt.setupFunc != nil {
				tt.setupFunc()
			}

			shouldRun, err := ShouldRunOnceAction(tt.action, tt.force)

			if tt.expectError {
				testutil.AssertError(t, err)
			} else {
				testutil.AssertNoError(t, err)
				testutil.AssertEqual(t, tt.shouldRun, shouldRun)
			}
		})
	}
}

func TestFilterRunOnceActions(t *testing.T) {
	// Setup test environment
	tmpDir := testutil.TempDir(t, "filter-test")
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	_ = os.Setenv("DODOT_DATA_DIR", tmpDir)
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	// Create sentinel directories and files
	homebrewDir := filepath.Join(tmpDir, "homebrew")
	installDir := filepath.Join(tmpDir, "install")
	_ = os.MkdirAll(homebrewDir, 0755)
	_ = os.MkdirAll(installDir, 0755)
	_ = os.WriteFile(filepath.Join(homebrewDir, "tools"), []byte("brew123"), 0644)
	_ = os.WriteFile(filepath.Join(installDir, "dev"), []byte("install456"), 0644)

	actions := []types.Action{
		// Regular action - always included
		{
			Type:        types.ActionTypeLink,
			Description: "Link action",
		},
		// Brew action - already executed
		{
			Type:        types.ActionTypeBrew,
			Description: "Brew tools",
			Pack:        "tools",
			Metadata: map[string]interface{}{
				"checksum": "brew123",
				"pack":     "tools",
			},
		},
		// Brew action - not executed
		{
			Type:        types.ActionTypeBrew,
			Description: "Brew utils",
			Pack:        "utils",
			Metadata: map[string]interface{}{
				"checksum": "brew789",
				"pack":     "utils",
			},
		},
		// Install action - already executed
		{
			Type:        types.ActionTypeInstall,
			Description: "Install dev",
			Pack:        "dev",
			Metadata: map[string]interface{}{
				"checksum": "install456",
				"pack":     "dev",
			},
		},
		// Install action - checksum changed
		{
			Type:        types.ActionTypeInstall,
			Description: "Install dev updated",
			Pack:        "dev",
			Metadata: map[string]interface{}{
				"checksum": "install999",
				"pack":     "dev",
			},
		},
	}

	tests := []struct {
		name          string
		force         bool
		expectedCount int
		expectedDescs []string
	}{
		{
			name:          "normal_filtering",
			force:         false,
			expectedCount: 3,
			expectedDescs: []string{"Link action", "Brew utils", "Install dev updated"},
		},
		{
			name:          "force_includes_all",
			force:         true,
			expectedCount: 5,
			expectedDescs: []string{"Link action", "Brew tools", "Brew utils", "Install dev", "Install dev updated"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filtered, err := FilterRunOnceActions(actions, tt.force)
			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tt.expectedCount, len(filtered))

			// Check that we got the expected actions
			for i, desc := range tt.expectedDescs {
				if i < len(filtered) {
					testutil.AssertEqual(t, desc, filtered[i].Description)
				}
			}
		})
	}
}

func TestFilterRunOnceActions_Empty(t *testing.T) {
	filtered, err := FilterRunOnceActions([]types.Action{}, false)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 0, len(filtered))

	filtered, err = FilterRunOnceActions(nil, false)
	testutil.AssertNoError(t, err)
	testutil.AssertNil(t, filtered)
}

func TestCalculateActionChecksum(t *testing.T) {
	tmpDir := testutil.TempDir(t, "checksum-test")

	// Create test files
	brewfile := filepath.Join(tmpDir, "Brewfile")
	installScript := filepath.Join(tmpDir, "install.sh")

	_ = os.WriteFile(brewfile, []byte("brew \"git\""), 0644)
	_ = os.WriteFile(installScript, []byte("#!/bin/bash\necho install"), 0755)

	tests := []struct {
		name        string
		action      types.Action
		expectError bool
	}{
		{
			name: "brew_action_checksum",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: brewfile,
			},
			expectError: false,
		},
		{
			name: "install_action_checksum",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: installScript,
			},
			expectError: false,
		},
		{
			name: "missing_source",
			action: types.Action{
				Type: types.ActionTypeBrew,
			},
			expectError: true,
		},
		{
			name: "unsupported_action_type",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "/some/file",
			},
			expectError: true,
		},
		{
			name: "nonexistent_file",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "/nonexistent/file",
			},
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			checksum, err := CalculateActionChecksum(tt.action)

			if tt.expectError {
				testutil.AssertError(t, err)
				testutil.AssertEqual(t, "", checksum)
			} else {
				testutil.AssertNoError(t, err)
				testutil.AssertTrue(t, len(checksum) > 0, "Checksum should not be empty")
			}
		})
	}
}

// Benchmarks
func BenchmarkShouldRunOnceAction(b *testing.B) {
	tmpDir := b.TempDir()
	_ = os.Setenv("DODOT_DATA_DIR", tmpDir)

	homebrewDir := filepath.Join(tmpDir, "homebrew")
	_ = os.MkdirAll(homebrewDir, 0755)
	_ = os.WriteFile(filepath.Join(homebrewDir, "tools"), []byte("abc123"), 0644)

	action := types.Action{
		Type: types.ActionTypeBrew,
		Metadata: map[string]interface{}{
			"checksum": "abc123",
			"pack":     "tools",
		},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, _ = ShouldRunOnceAction(action, false)
	}
}

func BenchmarkFilterRunOnceActions(b *testing.B) {
	tmpDir := b.TempDir()
	_ = os.Setenv("DODOT_DATA_DIR", tmpDir)

	actions := make([]types.Action, 100)
	for i := 0; i < 100; i++ {
		switch i % 3 {
		case 0:
			actions[i] = types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"checksum": "checksum",
					"pack":     "pack",
				},
			}
		case 1:
			actions[i] = types.Action{
				Type: types.ActionTypeInstall,
				Metadata: map[string]interface{}{
					"checksum": "checksum",
					"pack":     "pack",
				},
			}
		default:
			actions[i] = types.Action{
				Type: types.ActionTypeLink,
			}
		}
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, _ = FilterRunOnceActions(actions, false)
	}
}
