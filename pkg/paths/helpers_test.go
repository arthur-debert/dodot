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
			subdir:   InstallDir,
			expected: p.InstallDir(),
		},
		{
			name:     "brewfile directory",
			subdir:   BrewfileDir,
			expected: p.BrewfileDir(),
		},
		{
			name:     "custom subdirectory",
			subdir:   "custom",
			expected: filepath.Join(p.DataDir(), "custom"),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.GetDataSubdir(tt.subdir)
			if result != tt.expected {
				t.Errorf("GetDataSubdir(%q) = %q, want %q", tt.subdir, result, tt.expected)
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
			result := p.GetDeployedSubdir(tt.subdir)
			if result != tt.expected {
				t.Errorf("GetDeployedSubdir(%q) = %q, want %q", tt.subdir, result, tt.expected)
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

	// Test GetDataSubdir consistency
	if p.GetDataSubdir("test") != filepath.Join(p.xdgData, "test") {
		t.Errorf("GetDataSubdir does not match direct filepath.Join")
	}

	// Test GetDeployedSubdir consistency
	if p.GetDeployedSubdir("test") != filepath.Join(p.GetDataSubdir(DeployedDir), "test") {
		t.Errorf("GetDeployedSubdir does not match expected path")
	}

	// Verify that nested paths work correctly
	deployedBase := p.DeployedDir()
	shellProfileViaHelper := p.GetDeployedSubdir("shell_profile")
	shellProfileViaMethod := p.ShellProfileDir()
	shellProfileDirect := filepath.Join(deployedBase, "shell_profile")

	if shellProfileViaHelper != shellProfileViaMethod {
		t.Errorf("GetDeployedSubdir result differs from direct method")
	}

	if shellProfileViaHelper != shellProfileDirect {
		t.Errorf("GetDeployedSubdir result differs from direct filepath.Join")
	}
}
