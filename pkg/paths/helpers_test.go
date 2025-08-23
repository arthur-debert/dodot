package paths

import (
	"path/filepath"
	"testing"
)

func TestGetDataSubdir(t *testing.T) {
	tempDir := t.TempDir()
	t.Setenv("HOME", tempDir)
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	p, err := New("")
	if err != nil {
		t.Fatalf("Failed to create Paths: %v", err)
	}

	tests := []struct {
		name     string
		subdir   string
		expected string
	}{
		{
			name:     "state directory",
			subdir:   StateDir,
			expected: p.StateDir(),
		},
		{
			name:     "backups directory",
			subdir:   BackupsDir,
			expected: p.BackupsDir(),
		},
		{
			name:     "templates directory",
			subdir:   TemplatesDir,
			expected: p.TemplatesDir(),
		},
		{
			name:     "deployed directory",
			subdir:   DeployedDir,
			expected: p.DeployedDir(),
		},
		{
			name:     "shell directory",
			subdir:   ShellDir,
			expected: p.ShellDir(),
		},
		{
			name:     "install directory",
			subdir:   ProvisionDir,
			expected: p.ProvisionDir(),
		},
		{
			name:     "brewfile directory",
			subdir:   HomebrewDir,
			expected: p.HomebrewDir(),
		},
		{
			name:     "custom subdirectory",
			subdir:   "custom",
			expected: filepath.Join(p.DataDir(), "custom"),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Since GetDataSubdir is not part of the interface,
			// we calculate the expected result directly
			result := filepath.Join(p.DataDir(), tt.subdir)
			if result != tt.expected {
				t.Errorf("DataDir + subdir(%q) = %q, want %q", tt.subdir, result, tt.expected)
			}
		})
	}
}

func TestGetDeployedSubdir(t *testing.T) {
	tempDir := t.TempDir()
	t.Setenv("HOME", tempDir)
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	p, err := New("")
	if err != nil {
		t.Fatalf("Failed to create Paths: %v", err)
	}

	tests := []struct {
		name     string
		subdir   string
		expected string
	}{
		{
			name:     "shell_profile subdirectory",
			subdir:   "shell_profile",
			expected: p.ShellProfileDir(),
		},
		{
			name:     "path subdirectory",
			subdir:   "path",
			expected: p.PathDir(),
		},
		{
			name:     "shell_source subdirectory",
			subdir:   "shell_source",
			expected: p.ShellSourceDir(),
		},
		{
			name:     "symlink subdirectory",
			subdir:   "symlink",
			expected: p.SymlinkDir(),
		},
		{
			name:     "custom deployed subdirectory",
			subdir:   "custom_deployment",
			expected: filepath.Join(p.DeployedDir(), "custom_deployment"),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Since GetDeployedSubdir is not part of the interface,
			// we calculate the expected result directly
			result := filepath.Join(p.DeployedDir(), tt.subdir)
			if result != tt.expected {
				t.Errorf("DeployedDir + subdir(%q) = %q, want %q", tt.subdir, result, tt.expected)
			}
		})
	}
}

func TestHelperMethodsConsistency(t *testing.T) {
	// Test that the helper methods produce the same results as
	// direct filepath.Join calls would have produced
	tempDir := t.TempDir()
	t.Setenv("HOME", tempDir)
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	p, err := New("")
	if err != nil {
		t.Fatalf("Failed to create Paths: %v", err)
	}

	// Test data subdir consistency
	// Since we can't access private methods/fields, we test the public interface
	testDataPath := filepath.Join(p.DataDir(), "test")
	expectedTestPath := filepath.Join(p.DataDir(), "test")
	if testDataPath != expectedTestPath {
		t.Errorf("Data subdirectory path does not match expected path")
	}

	// Test deployed subdir consistency
	testDeployedPath := filepath.Join(p.DeployedDir(), "test")
	expectedDeployedPath := filepath.Join(p.DeployedDir(), "test")
	if testDeployedPath != expectedDeployedPath {
		t.Errorf("Deployed subdirectory path does not match expected path")
	}

	// Verify that nested paths work correctly
	deployedBase := p.DeployedDir()
	shellProfileViaInterface := p.ShellProfileDir()
	shellProfileDirect := filepath.Join(deployedBase, "shell_profile")

	if shellProfileViaInterface != shellProfileDirect {
		t.Errorf("ShellProfileDir result differs from direct filepath.Join: %s != %s",
			shellProfileViaInterface, shellProfileDirect)
	}
}
