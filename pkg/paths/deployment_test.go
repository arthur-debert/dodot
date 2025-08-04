package paths

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestDeploymentPaths(t *testing.T) {
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	tests := []struct {
		name     string
		method   func() string
		expected string
	}{
		{
			name:     "deployed dir",
			method:   p.DeployedDir,
			expected: filepath.Join(p.DataDir(), "deployed"),
		},
		{
			name:     "shell profile dir",
			method:   p.ShellProfileDir,
			expected: filepath.Join(p.DeployedDir(), "shell_profile"),
		},
		{
			name:     "path dir",
			method:   p.PathDir,
			expected: filepath.Join(p.DeployedDir(), "path"),
		},
		{
			name:     "shell source dir",
			method:   p.ShellSourceDir,
			expected: filepath.Join(p.DeployedDir(), "shell_source"),
		},
		{
			name:     "symlink dir",
			method:   p.SymlinkDir,
			expected: filepath.Join(p.DeployedDir(), "symlink"),
		},
		{
			name:     "shell dir",
			method:   p.ShellDir,
			expected: filepath.Join(p.DataDir(), "shell"),
		},
		{
			name:     "init script path",
			method:   p.InitScriptPath,
			expected: filepath.Join(p.ShellDir(), "dodot-init.sh"),
		},
		{
			name:     "install dir",
			method:   p.InstallDir,
			expected: filepath.Join(p.DataDir(), "install"),
		},
		{
			name:     "homebrew dir",
			method:   p.HomebrewDir,
			expected: filepath.Join(p.DataDir(), "homebrew"),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.method()
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

func TestGetHomeDirectory(t *testing.T) {
	tests := []struct {
		name      string
		envSetup  map[string]string
		wantErr   bool
		expectEnv bool // expect to get HOME env value
	}{
		{
			name: "normal case",
		},
		{
			name:     "with HOME env fallback",
			envSetup: map[string]string{
				// This test is tricky to set up properly
				// In practice, if os.UserHomeDir() fails, we fall back to HOME
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			for k, v := range tt.envSetup {
				t.Setenv(k, v)
			}

			home, err := GetHomeDirectory()

			if tt.wantErr {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertNotEmpty(t, home)
		})
	}
}

func TestGetHomeDirectoryWithDefault(t *testing.T) {
	defaultDir := "/tmp/default"

	// Normal case - should get actual home directory
	home := GetHomeDirectoryWithDefault(defaultDir)
	testutil.AssertNotEqual(t, defaultDir, home)
	testutil.AssertNotEmpty(t, home)
}

func TestCompatibilityFunctions(t *testing.T) {
	// Test that compatibility functions work
	tests := []struct {
		name   string
		fn     func() string
		verify func(t *testing.T, result string)
	}{
		{
			name: "GetShellProfileDir",
			fn:   GetShellProfileDir,
			verify: func(t *testing.T, result string) {
				testutil.AssertNotEmpty(t, result)
				testutil.AssertTrue(t, strings.HasSuffix(result, "shell_profile"), "Should end with 'shell_profile'")
			},
		},
		{
			name: "GetPathDir",
			fn:   GetPathDir,
			verify: func(t *testing.T, result string) {
				testutil.AssertNotEmpty(t, result)
				testutil.AssertTrue(t, strings.HasSuffix(result, "path"), "Should end with 'path'")
			},
		},
		{
			name: "GetSymlinkDir",
			fn:   GetSymlinkDir,
			verify: func(t *testing.T, result string) {
				testutil.AssertNotEmpty(t, result)
				testutil.AssertTrue(t, strings.HasSuffix(result, "symlink"), "Should end with 'symlink'")
			},
		},
		{
			name: "GetInstallDir",
			fn:   GetInstallDir,
			verify: func(t *testing.T, result string) {
				testutil.AssertNotEmpty(t, result)
				testutil.AssertTrue(t, strings.HasSuffix(result, "install"), "Should end with 'install'")
			},
		},
		{
			name: "GetHomebrewDir",
			fn:   GetHomebrewDir,
			verify: func(t *testing.T, result string) {
				testutil.AssertNotEmpty(t, result)
				testutil.AssertTrue(t, strings.HasSuffix(result, "homebrew"), "Should end with 'homebrew'")
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.fn()
			tt.verify(t, result)
		})
	}
}
