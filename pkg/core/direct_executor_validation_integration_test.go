package core

import (
	"fmt"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// TestValidateSafePath tests the validateSafePath method
// This is an integration test because it creates directories and sets environment variables
func TestValidateSafePath(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "validate-safe-path")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")
	dataDir := filepath.Join(homeDir, ".local", "share", "dodot")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", dataDir)

	p, err := paths.New(dotfilesDir)
	testutil.AssertNoError(t, err)

	tests := []struct {
		name              string
		path              string
		allowHomeSymlinks bool
		expectError       bool
		errorMsg          string
	}{
		{
			name:              "path in dotfiles root",
			path:              filepath.Join(dotfilesDir, "vim", "vimrc"),
			allowHomeSymlinks: false,
			expectError:       false,
		},
		{
			name:              "path in data dir",
			path:              filepath.Join(dataDir, "state", "sentinel"),
			allowHomeSymlinks: false,
			expectError:       false,
		},
		{
			name:              "path in home with symlinks allowed",
			path:              filepath.Join(homeDir, ".vimrc"),
			allowHomeSymlinks: true,
			expectError:       false,
		},
		{
			name:              "path in home with symlinks disallowed",
			path:              filepath.Join(homeDir, ".vimrc"),
			allowHomeSymlinks: false,
			expectError:       true,
			errorMsg:          "outside dodot-controlled directories",
		},
		{
			name:              "path outside all safe directories",
			path:              "/tmp/evil",
			allowHomeSymlinks: false,
			expectError:       true,
			errorMsg:          "outside dodot-controlled directories",
		},
		{
			name:              "path with traversal attempt",
			path:              filepath.Join(dotfilesDir, "..", "..", "etc", "passwd"),
			allowHomeSymlinks: false,
			expectError:       true,
			errorMsg:          "outside dodot-controlled directories",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			opts := &DirectExecutorOptions{
				Paths:             p,
				AllowHomeSymlinks: tt.allowHomeSymlinks,
				Config:            config.Default(),
			}
			executor := NewDirectExecutor(opts)

			err := executor.validateSafePath(tt.path)
			if tt.expectError {
				testutil.AssertError(t, err)
				testutil.AssertContains(t, err.Error(), tt.errorMsg)
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}

// TestValidateNotSystemFile tests protection of critical system files
// This is an integration test because it creates directories and sets environment variables
func TestValidateNotSystemFile(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "validate-system-file")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)

	p, err := paths.New(dotfilesDir)
	testutil.AssertNoError(t, err)

	// Create custom config with protected paths
	cfg := config.Default()
	// The default config already has these protected paths

	tests := []struct {
		name        string
		path        string
		expectError bool
		errorMsg    string
	}{
		{
			name:        "SSH private key",
			path:        filepath.Join(homeDir, ".ssh", "id_rsa"),
			expectError: true,
			errorMsg:    "cannot modify protected system file: .ssh/id_rsa",
		},
		{
			name:        "AWS credentials",
			path:        filepath.Join(homeDir, ".aws", "credentials"),
			expectError: true,
			errorMsg:    "cannot modify protected system file: .aws/credentials",
		},
		{
			name:        "GPG private key",
			path:        filepath.Join(homeDir, ".gnupg", "secring.gpg"),
			expectError: true,
			errorMsg:    "cannot modify protected system file: .gnupg",
		},
		{
			name:        "regular dotfile",
			path:        filepath.Join(homeDir, ".vimrc"),
			expectError: false,
		},
		{
			name:        "SSH config is allowed",
			path:        filepath.Join(homeDir, ".ssh", "config"),
			expectError: false,
		},
		{
			name:        "with tilde expansion",
			path:        "~/.ssh/id_rsa",
			expectError: true,
			errorMsg:    "cannot modify protected system file: .ssh/id_rsa",
		},
	}

	opts := &DirectExecutorOptions{
		Paths:             p,
		AllowHomeSymlinks: true,
		Config:            cfg,
	}
	executor := NewDirectExecutor(opts)

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := executor.validateNotSystemFile(tt.path)
			if tt.expectError {
				testutil.AssertError(t, err)
				if err != nil {
					testutil.AssertContains(t, err.Error(), tt.errorMsg)
				}
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}

// TestValidateLinkAction tests symlink-specific validation
// This is an integration test because it creates directories and sets environment variables
func TestValidateLinkAction(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "validate-link")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")
	deployedDir := filepath.Join(homeDir, ".local", "share", "dodot", "deployed")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot/deployed")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	p, err := paths.New(dotfilesDir)
	testutil.AssertNoError(t, err)

	tests := []struct {
		name              string
		source            string
		target            string
		allowHomeSymlinks bool
		expectError       bool
		errorMsg          string
	}{
		{
			name:              "valid link from dotfiles to home",
			source:            filepath.Join(dotfilesDir, "vim", "vimrc"),
			target:            filepath.Join(homeDir, ".vimrc"),
			allowHomeSymlinks: true,
			expectError:       false,
		},
		{
			name:              "valid link from deployed to home",
			source:            filepath.Join(deployedDir, "script.sh"),
			target:            filepath.Join(homeDir, ".local", "bin", "script"),
			allowHomeSymlinks: true,
			expectError:       false,
		},
		{
			name:              "source outside dotfiles/deployed",
			source:            "/etc/passwd",
			target:            filepath.Join(homeDir, ".passwd"),
			allowHomeSymlinks: true,
			expectError:       true,
			errorMsg:          "outside dotfiles or deployed directory",
		},
		{
			name:              "target in home without permission",
			source:            filepath.Join(dotfilesDir, "vimrc"),
			target:            filepath.Join(homeDir, ".vimrc"),
			allowHomeSymlinks: false,
			expectError:       true,
			errorMsg:          "outside dodot-controlled directories",
		},
		{
			name:              "target is protected system file",
			source:            filepath.Join(dotfilesDir, "ssh_key"),
			target:            filepath.Join(homeDir, ".ssh", "id_rsa"),
			allowHomeSymlinks: true,
			expectError:       true,
			errorMsg:          "cannot modify protected system file",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			opts := &DirectExecutorOptions{
				Paths:             p,
				AllowHomeSymlinks: tt.allowHomeSymlinks,
				Config:            config.Default(),
			}
			executor := NewDirectExecutor(opts)

			err := executor.validateLinkAction(tt.source, tt.target)
			if tt.expectError {
				testutil.AssertError(t, err)
				testutil.AssertContains(t, err.Error(), tt.errorMsg)
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}

// TestValidateWriteAction tests write/append validation
// This is an integration test because it creates directories and sets environment variables
func TestValidateWriteAction(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "validate-write")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")
	dataDir := filepath.Join(homeDir, ".local", "share", "dodot")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", dataDir)

	p, err := paths.New(dotfilesDir)
	testutil.AssertNoError(t, err)

	opts := &DirectExecutorOptions{
		Paths:             p,
		AllowHomeSymlinks: false, // Write should only go to dodot directories
		Config:            config.Default(),
	}
	executor := NewDirectExecutor(opts)

	tests := []struct {
		name        string
		target      string
		expectError bool
		errorMsg    string
	}{
		{
			name:        "write to data directory",
			target:      filepath.Join(dataDir, "config.json"),
			expectError: false,
		},
		{
			name:        "write to dotfiles",
			target:      filepath.Join(dotfilesDir, "generated.conf"),
			expectError: false,
		},
		{
			name:        "write to home directory",
			target:      filepath.Join(homeDir, ".config"),
			expectError: true,
			errorMsg:    "outside dodot-controlled directories",
		},
		{
			name:        "write to protected system file",
			target:      filepath.Join(homeDir, ".ssh", "id_rsa"),
			expectError: true,
			errorMsg:    "cannot modify protected system file",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := executor.validateWriteAction(tt.target)
			if tt.expectError {
				testutil.AssertError(t, err)
				testutil.AssertContains(t, err.Error(), tt.errorMsg)
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}

// TestValidateActionShellProfileSpecialCase tests the special case for shell_profile append actions
// This is an integration test because it creates directories and sets environment variables
func TestValidateActionShellProfileSpecialCase(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "validate-shell-profile")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)

	p, err := paths.New(dotfilesDir)
	testutil.AssertNoError(t, err)

	// Test with AllowHomeSymlinks: true
	t.Run("with home symlinks allowed", func(t *testing.T) {
		opts := &DirectExecutorOptions{
			Paths:             p,
			AllowHomeSymlinks: true,
			Config:            config.Default(),
		}
		executor := NewDirectExecutor(opts)

		tests := []struct {
			name        string
			action      types.Action
			expectError bool
			errorMsg    string
		}{
			{
				name: "shell_profile append to .bashrc",
				action: types.Action{
					Type:        types.ActionTypeAppend,
					PowerUpName: "shell_profile",
					Target:      filepath.Join(homeDir, ".bashrc"),
				},
				expectError: false,
			},
			{
				name: "regular append to .bashrc",
				action: types.Action{
					Type:        types.ActionTypeAppend,
					PowerUpName: "other",
					Target:      filepath.Join(homeDir, ".bashrc"),
				},
				expectError: false, // With AllowHomeSymlinks: true, this is allowed
			},
			{
				name: "shell_profile append to protected file",
				action: types.Action{
					Type:        types.ActionTypeAppend,
					PowerUpName: "shell_profile",
					Target:      filepath.Join(homeDir, ".ssh", "id_rsa"),
				},
				expectError: true,
				errorMsg:    "cannot modify protected system file",
			},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				err := executor.validateAction(tt.action)
				if tt.expectError {
					testutil.AssertError(t, err)
					if err != nil {
						testutil.AssertContains(t, err.Error(), tt.errorMsg)
					}
				} else {
					testutil.AssertNoError(t, err)
				}
			})
		}
	})

	// Test with AllowHomeSymlinks: false - this is where the special case matters
	t.Run("without home symlinks allowed", func(t *testing.T) {
		opts := &DirectExecutorOptions{
			Paths:             p,
			AllowHomeSymlinks: false,
			Config:            config.Default(),
		}
		executor := NewDirectExecutor(opts)

		tests := []struct {
			name        string
			action      types.Action
			expectError bool
			errorMsg    string
		}{
			{
				name: "shell_profile append to .bashrc STILL ALLOWED",
				action: types.Action{
					Type:        types.ActionTypeAppend,
					PowerUpName: "shell_profile",
					Target:      filepath.Join(homeDir, ".bashrc"),
				},
				expectError: false, // Special case allows this
			},
			{
				name: "regular append to .bashrc blocked",
				action: types.Action{
					Type:        types.ActionTypeAppend,
					PowerUpName: "other",
					Target:      filepath.Join(homeDir, ".bashrc"),
				},
				expectError: true,
				errorMsg:    "outside dodot-controlled directories",
			},
			{
				name: "shell_profile write blocked",
				action: types.Action{
					Type:        types.ActionTypeWrite,
					PowerUpName: "shell_profile",
					Target:      filepath.Join(homeDir, ".bashrc"),
				},
				expectError: true,
				errorMsg:    "outside dodot-controlled directories",
			},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				err := executor.validateAction(tt.action)
				if tt.expectError {
					testutil.AssertError(t, err)
					if err != nil {
						testutil.AssertContains(t, err.Error(), tt.errorMsg)
					}
				} else {
					testutil.AssertNoError(t, err)
				}
			})
		}
	})
}

// TestValidationIntegrationWithRealPowerUps tests validation with real-world powerup scenarios
// This is an integration test because it creates files and directories
func TestValidationIntegrationWithRealPowerUps(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "validate-integration")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create some test files
	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "set number")
	testutil.CreateFile(t, dotfilesDir, "bash/bashrc", "export PATH")
	testutil.CreateFile(t, dotfilesDir, "ssh/config", "Host *")

	p, err := paths.New(dotfilesDir)
	testutil.AssertNoError(t, err)

	opts := &DirectExecutorOptions{
		Paths:             p,
		AllowHomeSymlinks: true,
		Config:            config.Default(),
	}
	executor := NewDirectExecutor(opts)

	validActions := []types.Action{
		// Symlink powerup
		{
			Type:        types.ActionTypeLink,
			PowerUpName: "symlink",
			Source:      filepath.Join(dotfilesDir, "vim", "vimrc"),
			Target:      filepath.Join(homeDir, ".vimrc"),
		},
		// Shell profile powerup
		{
			Type:        types.ActionTypeAppend,
			PowerUpName: "shell_profile",
			Target:      filepath.Join(homeDir, ".bashrc"),
			Content:     "source " + filepath.Join(dotfilesDir, "bash", "bashrc"),
		},
		// Install script
		{
			Type:        types.ActionTypeCopy,
			PowerUpName: "install_script",
			Source:      filepath.Join(dotfilesDir, "install.sh"),
			Target:      filepath.Join(p.InstallDir(), "install.sh"),
		},
	}

	invalidActions := []types.Action{
		// Try to symlink system file
		{
			Type:        types.ActionTypeLink,
			PowerUpName: "symlink",
			Source:      filepath.Join(dotfilesDir, "ssh", "config"),
			Target:      filepath.Join(homeDir, ".ssh", "id_rsa"),
		},
		// Try to write outside safe directories
		{
			Type:        types.ActionTypeWrite,
			PowerUpName: "config",
			Target:      "/etc/passwd",
			Content:     "evil",
		},
		// Try to link from outside dotfiles
		{
			Type:        types.ActionTypeLink,
			PowerUpName: "symlink",
			Source:      "/etc/passwd",
			Target:      filepath.Join(homeDir, ".passwd"),
		},
	}

	// Test valid actions
	for i, action := range validActions {
		t.Run(fmt.Sprintf("valid_action_%d", i), func(t *testing.T) {
			err := executor.validateAction(action)
			testutil.AssertNoError(t, err)
		})
	}

	// Test invalid actions
	for i, action := range invalidActions {
		t.Run(fmt.Sprintf("invalid_action_%d", i), func(t *testing.T) {
			err := executor.validateAction(action)
			testutil.AssertError(t, err)
		})
	}
}
