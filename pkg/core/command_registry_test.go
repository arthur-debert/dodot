package core

import (
	"fmt"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/display"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCommandRegistry(t *testing.T) {
	// Save original registry and restore after tests
	originalRegistry := CommandRegistry
	defer func() {
		CommandRegistry = originalRegistry
	}()

	t.Run("RegisterCommand adds command to registry", func(t *testing.T) {
		CommandRegistry = make(map[string]CommandConfig)

		testCommand := CommandConfig{
			Name: "test-command",
			Type: SimpleExecution,
			Execute: func(opts CommandExecuteOptions) (*display.PackCommandResult, error) {
				return &display.PackCommandResult{
					Command:   "test-command",
					Timestamp: time.Now(),
				}, nil
			},
			Validators: []ValidatorFunc{},
		}

		RegisterCommand(testCommand)

		assert.Contains(t, CommandRegistry, "test-command")
		assert.Equal(t, testCommand.Name, CommandRegistry["test-command"].Name)
	})

	t.Run("ExecuteRegisteredCommand with unknown command", func(t *testing.T) {
		CommandRegistry = make(map[string]CommandConfig)

		_, err := ExecuteRegisteredCommand("unknown-command", CommandExecuteOptions{})
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "unknown command: unknown-command")
	})

	t.Run("ExecuteRegisteredCommand with simple command", func(t *testing.T) {
		CommandRegistry = make(map[string]CommandConfig)
		fs := testutil.NewMemoryFS()

		// Create test pack structure
		require.NoError(t, fs.MkdirAll("/dotfiles/pack1", 0755))

		executeCalled := false
		RegisterCommand(CommandConfig{
			Name: "simple-test",
			Type: SimpleExecution,
			Execute: func(opts CommandExecuteOptions) (*display.PackCommandResult, error) {
				executeCalled = true
				return &display.PackCommandResult{
					Command:   "simple-test",
					Timestamp: time.Now(),
					Packs: []display.DisplayPack{
						{Name: "pack1"},
					},
				}, nil
			},
			Validators: []ValidatorFunc{},
		})

		result, err := ExecuteRegisteredCommand("simple-test", CommandExecuteOptions{
			DotfilesRoot: "/dotfiles",
			PackNames:    []string{"pack1"},
			FileSystem:   fs,
		})

		assert.NoError(t, err)
		assert.True(t, executeCalled)
		assert.Equal(t, "simple-test", result.Command)
		assert.Len(t, result.Packs, 1)
	})

	t.Run("ExecuteRegisteredCommand with validator failure", func(t *testing.T) {
		CommandRegistry = make(map[string]CommandConfig)
		fs := testutil.NewMemoryFS()

		// Create test pack
		require.NoError(t, fs.MkdirAll("/dotfiles/pack1", 0755))

		RegisterCommand(CommandConfig{
			Name: "validated-test",
			Type: SimpleExecution,
			Execute: func(opts CommandExecuteOptions) (*display.PackCommandResult, error) {
				return &display.PackCommandResult{
					Command: "validated-test",
				}, nil
			},
			Validators: []ValidatorFunc{
				func(packs []types.Pack, opts CommandExecuteOptions) error {
					return fmt.Errorf("validation failed")
				},
			},
		})

		_, err := ExecuteRegisteredCommand("validated-test", CommandExecuteOptions{
			DotfilesRoot: "/dotfiles",
			PackNames:    []string{"pack1"},
			FileSystem:   fs,
		})

		assert.Error(t, err)
		assert.Contains(t, err.Error(), "validation failed")
	})

	t.Run("ExecuteRegisteredCommand with init command special handling", func(t *testing.T) {
		CommandRegistry = make(map[string]CommandConfig)
		fs := testutil.NewMemoryFS()

		// Don't create pack - init should work with non-existent packs
		require.NoError(t, fs.MkdirAll("/dotfiles", 0755))

		packsPassedToValidator := -1
		RegisterCommand(CommandConfig{
			Name: "init",
			Type: SimpleExecution,
			Execute: func(opts CommandExecuteOptions) (*display.PackCommandResult, error) {
				return &display.PackCommandResult{
					Command: "init",
				}, nil
			},
			Validators: []ValidatorFunc{
				func(packs []types.Pack, opts CommandExecuteOptions) error {
					packsPassedToValidator = len(packs)
					return nil
				},
			},
		})

		_, err := ExecuteRegisteredCommand("init", CommandExecuteOptions{
			DotfilesRoot: "/dotfiles",
			PackNames:    []string{"newpack"},
			FileSystem:   fs,
		})

		assert.NoError(t, err)
		assert.Equal(t, 0, packsPassedToValidator, "init command should receive empty pack list")
	})
}

func TestValidators(t *testing.T) {
	fs := testutil.NewMemoryFS()

	t.Run("ValidateSinglePack", func(t *testing.T) {
		tests := []struct {
			name      string
			packs     []types.Pack
			wantError bool
		}{
			{
				name:      "single pack - valid",
				packs:     []types.Pack{{Name: "pack1"}},
				wantError: false,
			},
			{
				name:      "no packs - invalid",
				packs:     []types.Pack{},
				wantError: true,
			},
			{
				name:      "multiple packs - invalid",
				packs:     []types.Pack{{Name: "pack1"}, {Name: "pack2"}},
				wantError: true,
			},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				err := ValidateSinglePack(tt.packs, CommandExecuteOptions{})
				if tt.wantError {
					assert.Error(t, err)
				} else {
					assert.NoError(t, err)
				}
			})
		}
	})

	t.Run("ValidatePackDoesNotExist", func(t *testing.T) {
		// Create existing pack
		require.NoError(t, fs.MkdirAll("/dotfiles/existing", 0755))

		tests := []struct {
			name      string
			packNames []string
			wantError bool
			errorMsg  string
		}{
			{
				name:      "non-existent pack - valid",
				packNames: []string{"newpack"},
				wantError: false,
			},
			{
				name:      "existing pack - invalid",
				packNames: []string{"existing"},
				wantError: true,
				errorMsg:  "already exists",
			},
			{
				name:      "no pack name - invalid",
				packNames: []string{},
				wantError: true,
				errorMsg:  "pack name is required",
			},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				err := ValidatePackDoesNotExist(nil, CommandExecuteOptions{
					DotfilesRoot: "/dotfiles",
					PackNames:    tt.packNames,
					FileSystem:   fs,
				})
				if tt.wantError {
					assert.Error(t, err)
					if tt.errorMsg != "" {
						assert.Contains(t, err.Error(), tt.errorMsg)
					}
				} else {
					assert.NoError(t, err)
				}
			})
		}
	})

	t.Run("ValidateFileExists", func(t *testing.T) {
		// Create test file and directory
		require.NoError(t, fs.WriteFile("/testfile.txt", []byte("content"), 0644))
		require.NoError(t, fs.MkdirAll("/testdir", 0755))

		tests := []struct {
			name      string
			options   map[string]interface{}
			wantError bool
			errorMsg  string
		}{
			{
				name:      "existing file - valid",
				options:   map[string]interface{}{"file": "/testfile.txt"},
				wantError: false,
			},
			{
				name:      "non-existent file - invalid",
				options:   map[string]interface{}{"file": "/nonexistent.txt"},
				wantError: true,
				errorMsg:  "file does not exist",
			},
			{
				name:      "directory instead of file - invalid",
				options:   map[string]interface{}{"file": "/testdir"},
				wantError: true,
				errorMsg:  "path is a directory",
			},
			{
				name:      "no file option - invalid",
				options:   map[string]interface{}{},
				wantError: true,
				errorMsg:  "file path is required",
			},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				err := ValidateFileExists(nil, CommandExecuteOptions{
					FileSystem: fs,
					Options:    tt.options,
				})
				if tt.wantError {
					assert.Error(t, err)
					if tt.errorMsg != "" {
						assert.Contains(t, err.Error(), tt.errorMsg)
					}
				} else {
					assert.NoError(t, err)
				}
			})
		}
	})
}
