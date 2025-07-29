package types

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestGetDodotDataDir(t *testing.T) {
	// Save original env vars
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	origXDGDataHome := os.Getenv("XDG_DATA_HOME")
	origHome := os.Getenv("HOME")
	
	// Restore after test
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
		_ = os.Setenv("XDG_DATA_HOME", origXDGDataHome)
		_ = os.Setenv("HOME", origHome)
	})

	tests := []struct {
		name         string
		dodotDataDir string
		xdgDataHome  string
		homeDir      string
		expected     string
	}{
		{
			name:         "DODOT_DATA_DIR_set",
			dodotDataDir: "/custom/dodot/data",
			xdgDataHome:  "/home/user/.local/share",
			homeDir:      "/home/user",
			expected:     "/custom/dodot/data",
		},
		{
			name:         "XDG_DATA_HOME_set",
			dodotDataDir: "",
			xdgDataHome:  "/home/user/.local/share",
			homeDir:      "/home/user",
			expected:     "/home/user/.local/share/dodot",
		},
		{
			name:         "fallback_to_home",
			dodotDataDir: "",
			xdgDataHome:  "",
			homeDir:      "/home/user",
			expected:     "/home/user/.local/share/dodot",
		},
		{
			name:         "all_unset_uses_user_home",
			dodotDataDir: "",
			xdgDataHome:  "",
			homeDir:      "",
			expected:     "", // Will be set dynamically in test
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Set environment variables
			_ = os.Setenv("DODOT_DATA_DIR", tt.dodotDataDir)
			_ = os.Setenv("XDG_DATA_HOME", tt.xdgDataHome)
			if tt.homeDir != "" {
				_ = os.Setenv("HOME", tt.homeDir)
			}

			result := GetDodotDataDir()
			
			// For the dynamic test case, calculate expected based on actual home
			if tt.name == "all_unset_uses_user_home" {
				actualHome, _ := os.UserHomeDir()
				tt.expected = filepath.Join(actualHome, ".local", "share", "dodot")
			}
			
			// Handle Windows path differences
			if runtime.GOOS == "windows" {
				result = strings.ReplaceAll(result, "\\", "/")
				tt.expected = strings.ReplaceAll(tt.expected, "\\", "/")
			}
			
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

func TestGetDeployedDir(t *testing.T) {
	// Save and restore env
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	tests := []struct {
		name     string
		dataDir  string
		expected string
	}{
		{
			name:     "custom_data_dir",
			dataDir:  "/custom/data",
			expected: "/custom/data/deployed",
		},
		{
			name:     "default_data_dir",
			dataDir:  "",
			expected: func() string {
				return filepath.Join(GetDodotDataDir(), "deployed")
			}(),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_ = os.Setenv("DODOT_DATA_DIR", tt.dataDir)
			result := GetDeployedDir()
			
			if runtime.GOOS == "windows" {
				result = strings.ReplaceAll(result, "\\", "/")
				tt.expected = strings.ReplaceAll(tt.expected, "\\", "/")
			}
			
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

func TestGetSymlinkDir(t *testing.T) {
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	_ = os.Setenv("DODOT_DATA_DIR", "/test/data")
	expected := "/test/data/deployed/symlink"
	result := GetSymlinkDir()
	
	if runtime.GOOS == "windows" {
		result = strings.ReplaceAll(result, "\\", "/")
		expected = strings.ReplaceAll(expected, "\\", "/")
	}
	
	testutil.AssertEqual(t, expected, result)
}

func TestGetShellProfileDir(t *testing.T) {
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	_ = os.Setenv("DODOT_DATA_DIR", "/test/data")
	expected := "/test/data/deployed/shell_profile"
	result := GetShellProfileDir()
	
	if runtime.GOOS == "windows" {
		result = strings.ReplaceAll(result, "\\", "/")
		expected = strings.ReplaceAll(expected, "\\", "/")
	}
	
	testutil.AssertEqual(t, expected, result)
}

func TestGetPathDir(t *testing.T) {
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	_ = os.Setenv("DODOT_DATA_DIR", "/test/data")
	expected := "/test/data/deployed/path"
	result := GetPathDir()
	
	if runtime.GOOS == "windows" {
		result = strings.ReplaceAll(result, "\\", "/")
		expected = strings.ReplaceAll(expected, "\\", "/")
	}
	
	testutil.AssertEqual(t, expected, result)
}

func TestGetShellSourceDir(t *testing.T) {
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	_ = os.Setenv("DODOT_DATA_DIR", "/test/data")
	expected := "/test/data/deployed/shell_source"
	result := GetShellSourceDir()
	
	if runtime.GOOS == "windows" {
		result = strings.ReplaceAll(result, "\\", "/")
		expected = strings.ReplaceAll(expected, "\\", "/")
	}
	
	testutil.AssertEqual(t, expected, result)
}

func TestGetShellDir(t *testing.T) {
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	_ = os.Setenv("DODOT_DATA_DIR", "/test/data")
	expected := "/test/data/shell"
	result := GetShellDir()
	
	if runtime.GOOS == "windows" {
		result = strings.ReplaceAll(result, "\\", "/")
		expected = strings.ReplaceAll(expected, "\\", "/")
	}
	
	testutil.AssertEqual(t, expected, result)
}

func TestGetInitScriptPath(t *testing.T) {
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	_ = os.Setenv("DODOT_DATA_DIR", "/test/data")
	expected := "/test/data/shell/dodot-init.sh"
	result := GetInitScriptPath()
	
	if runtime.GOOS == "windows" {
		result = strings.ReplaceAll(result, "\\", "/")
		expected = strings.ReplaceAll(expected, "\\", "/")
	}
	
	testutil.AssertEqual(t, expected, result)
}

// Test all directories with consistent data dir
func TestAllDirectoriesConsistent(t *testing.T) {
	origDataDir := os.Getenv("DODOT_DATA_DIR")
	t.Cleanup(func() {
		_ = os.Setenv("DODOT_DATA_DIR", origDataDir)
	})

	baseDir := "/test/dodot"
	_ = os.Setenv("DODOT_DATA_DIR", baseDir)

	// All deployed subdirectories should be under deployed/
	deployedBase := filepath.Join(baseDir, "deployed")
	
	tests := []struct {
		name     string
		fn       func() string
		expected string
	}{
		{"GetDeployedDir", GetDeployedDir, deployedBase},
		{"GetSymlinkDir", GetSymlinkDir, filepath.Join(deployedBase, "symlink")},
		{"GetShellProfileDir", GetShellProfileDir, filepath.Join(deployedBase, "shell_profile")},
		{"GetPathDir", GetPathDir, filepath.Join(deployedBase, "path")},
		{"GetShellSourceDir", GetShellSourceDir, filepath.Join(deployedBase, "shell_source")},
		{"GetShellDir", GetShellDir, filepath.Join(baseDir, "shell")},
		{"GetInitScriptPath", GetInitScriptPath, filepath.Join(baseDir, "shell", "dodot-init.sh")},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.fn()
			
			if runtime.GOOS == "windows" {
				result = strings.ReplaceAll(result, "\\", "/")
				tt.expected = strings.ReplaceAll(tt.expected, "\\", "/")
			}
			
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

// Test that paths are absolute
func TestPathsAreAbsolute(t *testing.T) {
	// All path functions should return absolute paths
	paths := []string{
		GetDodotDataDir(),
		GetDeployedDir(),
		GetSymlinkDir(),
		GetShellProfileDir(),
		GetPathDir(),
		GetShellSourceDir(),
		GetShellDir(),
		GetInitScriptPath(),
	}

	for i, path := range paths {
		testutil.AssertTrue(t, filepath.IsAbs(path), 
			"Path %d should be absolute: %s", i, path)
	}
}

// Benchmarks
func BenchmarkGetDodotDataDir(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = GetDodotDataDir()
	}
}

func BenchmarkGetDeployedDir(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = GetDeployedDir()
	}
}

func BenchmarkAllPathFunctions(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = GetDodotDataDir()
		_ = GetDeployedDir()
		_ = GetSymlinkDir()
		_ = GetShellProfileDir()
		_ = GetPathDir()
		_ = GetShellSourceDir()
		_ = GetShellDir()
		_ = GetInitScriptPath()
	}
}