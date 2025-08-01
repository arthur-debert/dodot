package core

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	doerrors "github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetFileOperations(t *testing.T) {
	tests := []struct {
		name         string
		actions      []types.Action
		wantOpsCount int
		checkOps     func(t *testing.T, ops []types.Operation)
		wantError    bool
	}{
		{
			name:         "empty_actions",
			actions:      []types.Action{},
			wantOpsCount: 0,
			wantError:    false,
		},
		{
			name:         "nil_actions",
			actions:      nil,
			wantOpsCount: 0,
			wantError:    false,
		},
		{
			name: "single_link_action",
			actions: []types.Action{
				{
					Type:        types.ActionTypeLink,
					Description: "Link config file",
					Source:      "/source/config.yml",
					Target:      "~/.config/app/config.yml",
					Pack:        "app",
					Priority:    100,
				},
			},
			wantOpsCount: 3, // mkdir parent, deploy symlink, user symlink
			checkOps: func(t *testing.T, ops []types.Operation) {
				// Should create parent directory first
				testutil.AssertEqual(t, types.OperationCreateDir, ops[0].Type)
				testutil.AssertContains(t, ops[0].Target, ".config/app")

				// Deploy symlink
				testutil.AssertEqual(t, types.OperationCreateSymlink, ops[1].Type)
				testutil.AssertEqual(t, "/source/config.yml", ops[1].Source)

				// User symlink
				testutil.AssertEqual(t, types.OperationCreateSymlink, ops[2].Type)
			},
		},
		{
			name: "multiple_actions_sorted_by_priority",
			actions: []types.Action{
				{
					Type:     types.ActionTypeLink,
					Source:   "/source/low",
					Target:   "~/low",
					Priority: 10,
				},
				{
					Type:     types.ActionTypeLink,
					Source:   "/source/high",
					Target:   "~/high",
					Priority: 100,
				},
				{
					Type:     types.ActionTypeLink,
					Source:   "/source/medium",
					Target:   "~/medium",
					Priority: 50,
				},
			},
			wantOpsCount: 7, // 1 parent dir (deduplicated) + 6 symlink ops (2 per link)
			checkOps: func(t *testing.T, ops []types.Operation) {
				// Should have one directory creation for home
				dirCount := 0
				for _, op := range ops {
					if op.Type == types.OperationCreateDir {
						dirCount++
					}
				}
				testutil.AssertEqual(t, 1, dirCount)
				
				// Find deploy symlink operations and check order
				var deployOps []types.Operation
				for _, op := range ops {
					if op.Type == types.OperationCreateSymlink && strings.Contains(op.Target, "deployed/symlink") {
						deployOps = append(deployOps, op)
					}
				}
				testutil.AssertEqual(t, 3, len(deployOps))
				
				// High priority should be processed first
				testutil.AssertEqual(t, "/source/high", deployOps[0].Source)
				// Medium priority second
				testutil.AssertEqual(t, "/source/medium", deployOps[1].Source)
				// Low priority last
				testutil.AssertEqual(t, "/source/low", deployOps[2].Source)
			},
		},
		{
			name: "action_conversion_error",
			actions: []types.Action{
				{
					Type:   types.ActionTypeLink,
					Source: "", // Missing source
					Target: "~/target",
				},
			},
			wantError: true,
		},
		{
			name: "run_action_returns_no_ops",
			actions: []types.Action{
				{
					Type:    types.ActionTypeRun,
					Command: "echo",
					Args:    []string{"hello"},
				},
			},
			wantOpsCount: 0,
			wantError:    false,
		},
		{
			name: "brew_and_install_actions",
			actions: []types.Action{
				{
					Type:     types.ActionTypeBrew,
					Source:   "/packs/tools/Brewfile",
					Priority: 10,
					Metadata: map[string]interface{}{
						"checksum": "brew123",
						"pack":     "tools",
					},
				},
				{
					Type:     types.ActionTypeInstall,
					Source:   "/packs/dev/install.sh",
					Priority: 20,
					Metadata: map[string]interface{}{
						"checksum": "install456",
						"pack":     "dev",
					},
				},
			},
			wantOpsCount: 4, // 2 ops per action (create dir + write sentinel)
			checkOps: func(t *testing.T, ops []types.Operation) {
				// Install action should be processed first (higher priority)
				testutil.AssertEqual(t, types.OperationCreateDir, ops[0].Type)
				testutil.AssertEqual(t, paths.GetInstallDir(), ops[0].Target)

				testutil.AssertEqual(t, types.OperationWriteFile, ops[1].Type)
				testutil.AssertContains(t, ops[1].Target, "dev")
				testutil.AssertEqual(t, "install456", ops[1].Content)

				// Then brew action
				testutil.AssertEqual(t, types.OperationCreateDir, ops[2].Type)
				testutil.AssertEqual(t, paths.GetBrewfileDir(), ops[2].Target)

				testutil.AssertEqual(t, types.OperationWriteFile, ops[3].Type)
				testutil.AssertContains(t, ops[3].Target, "tools")
				testutil.AssertEqual(t, "brew123", ops[3].Content)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ops, err := GetFileOperations(tt.actions)

			if tt.wantError {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tt.wantOpsCount, len(ops))

			if tt.checkOps != nil {
				tt.checkOps(t, ops)
			}
		})
	}
}

func TestConvertAction(t *testing.T) {
	homeDir, _ := os.UserHomeDir()

	tests := []struct {
		name      string
		action    types.Action
		wantOps   []types.Operation
		wantError bool
		errorCode doerrors.ErrorCode
	}{
		// Link action tests
		{
			name: "link_action_success",
			action: types.Action{
				Type:        types.ActionTypeLink,
				Description: "Link vimrc",
				Source:      "/dotfiles/vim/.vimrc",
				Target:      "~/.vimrc",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      homeDir,
					Description: "Create parent directory for .vimrc",
				},
				{
					Type:        types.OperationCreateSymlink,
					Source:      "/dotfiles/vim/.vimrc",
					Target:      filepath.Join(paths.GetSymlinkDir(), ".vimrc"),
					Description: "Deploy symlink for .vimrc",
				},
				{
					Type:        types.OperationCreateSymlink,
					Source:      filepath.Join(paths.GetSymlinkDir(), ".vimrc"),
					Target:      filepath.Join(homeDir, ".vimrc"),
					Description: "Link vimrc",
				},
			},
		},
		{
			name: "link_action_with_parent_dir",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "/source/config.yml",
				Target: "~/.config/app/config.yml",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      filepath.Join(homeDir, ".config/app"),
					Description: "Create parent directory for config.yml",
				},
				{
					Type:        types.OperationCreateSymlink,
					Source:      "/source/config.yml",
					Target:      filepath.Join(paths.GetSymlinkDir(), "config.yml"),
					Description: "Deploy symlink for config.yml",
				},
				{
					Type:        types.OperationCreateSymlink,
					Source:      filepath.Join(paths.GetSymlinkDir(), "config.yml"),
					Target:      filepath.Join(homeDir, ".config/app/config.yml"),
					Description: "",
				},
			},
		},
		{
			name: "link_action_missing_source",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "",
				Target: "~/target",
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		{
			name: "link_action_missing_target",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "/source",
				Target: "",
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		// Copy action tests
		{
			name: "copy_action_success",
			action: types.Action{
				Type:        types.ActionTypeCopy,
				Description: "Copy template",
				Source:      "/templates/gitconfig",
				Target:      "~/.gitconfig",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      homeDir,
					Description: "Create parent directory for .gitconfig",
				},
				{
					Type:        types.OperationCopyFile,
					Source:      "/templates/gitconfig",
					Target:      filepath.Join(homeDir, ".gitconfig"),
					Description: "Copy template",
				},
			},
		},
		{
			name: "copy_action_with_parent_dir",
			action: types.Action{
				Type:   types.ActionTypeCopy,
				Source: "/source/data.json",
				Target: "~/.config/app/data.json",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      filepath.Join(homeDir, ".config/app"),
					Description: "Create parent directory for data.json",
				},
				{
					Type:        types.OperationCopyFile,
					Source:      "/source/data.json",
					Target:      filepath.Join(homeDir, ".config/app/data.json"),
					Description: "",
				},
			},
		},
		{
			name: "copy_action_missing_source",
			action: types.Action{
				Type:   types.ActionTypeCopy,
				Target: "~/target",
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		// Write action tests
		{
			name: "write_action_success",
			action: types.Action{
				Type:        types.ActionTypeWrite,
				Description: "Create config",
				Target:      "~/.myapp.conf",
				Content:     "# My App Config\nkey=value",
				Mode:        0644,
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      homeDir,
					Description: "Create parent directory for .myapp.conf",
				},
				{
					Type:        types.OperationWriteFile,
					Target:      filepath.Join(homeDir, ".myapp.conf"),
					Content:     "# My App Config\nkey=value",
					Mode:        uint32Ptr(0644),
					Description: "Create config",
				},
			},
		},
		{
			name: "write_action_no_mode",
			action: types.Action{
				Type:    types.ActionTypeWrite,
				Target:  "~/file.txt",
				Content: "content",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      homeDir,
					Description: "Create parent directory for file.txt",
				},
				{
					Type:        types.OperationWriteFile,
					Target:      filepath.Join(homeDir, "file.txt"),
					Content:     "content",
					Mode:        nil,
					Description: "",
				},
			},
		},
		{
			name: "write_action_missing_target",
			action: types.Action{
				Type:    types.ActionTypeWrite,
				Content: "content",
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		// Append action tests
		{
			name: "append_action_success",
			action: types.Action{
				Type:    types.ActionTypeAppend,
				Target:  "~/.bashrc",
				Content: "\n# Added by dodot\nexport FOO=bar",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationWriteFile,
					Target:      filepath.Join(homeDir, ".bashrc"),
					Content:     "\n# Added by dodot\nexport FOO=bar",
					Description: "Append to ~/.bashrc",
				},
			},
		},
		{
			name: "append_action_missing_target",
			action: types.Action{
				Type:    types.ActionTypeAppend,
				Content: "content",
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		// Mkdir action tests
		{
			name: "mkdir_action_success",
			action: types.Action{
				Type:        types.ActionTypeMkdir,
				Description: "Create app dir",
				Target:      "~/.config/myapp",
				Mode:        0755,
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      filepath.Join(homeDir, ".config/myapp"),
					Mode:        uint32Ptr(0755),
					Description: "Create app dir",
				},
			},
		},
		{
			name: "mkdir_action_no_mode",
			action: types.Action{
				Type:   types.ActionTypeMkdir,
				Target: "~/somedir",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      filepath.Join(homeDir, "somedir"),
					Mode:        nil,
					Description: "",
				},
			},
		},
		{
			name: "mkdir_action_missing_target",
			action: types.Action{
				Type: types.ActionTypeMkdir,
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		// Shell source action tests
		{
			name: "shell_source_action_success",
			action: types.Action{
				Type:   types.ActionTypeShellSource,
				Source: "/dotfiles/shell/aliases.sh",
				Pack:   "shell",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      paths.GetShellProfileDir(),
					Description: "Create shell profile deployment directory",
				},
				{
					Type:        types.OperationCreateSymlink,
					Source:      "/dotfiles/shell/aliases.sh",
					Target:      filepath.Join(paths.GetShellProfileDir(), "shell.sh"),
					Description: "Deploy shell profile script from shell",
				},
			},
		},
		{
			name: "shell_source_action_no_pack",
			action: types.Action{
				Type:   types.ActionTypeShellSource,
				Source: "/dotfiles/custom.sh",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      paths.GetShellProfileDir(),
					Description: "Create shell profile deployment directory",
				},
				{
					Type:        types.OperationCreateSymlink,
					Source:      "/dotfiles/custom.sh",
					Target:      filepath.Join(paths.GetShellProfileDir(), "custom.sh"),
					Description: "Deploy shell profile script from ",
				},
			},
		},
		{
			name: "shell_source_action_missing_source",
			action: types.Action{
				Type: types.ActionTypeShellSource,
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		// Path add action tests
		{
			name: "path_add_action_success",
			action: types.Action{
				Type:   types.ActionTypePathAdd,
				Source: "/dotfiles/bin",
				Pack:   "tools",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      paths.GetPathDir(),
					Description: "Create PATH deployment directory",
				},
				{
					Type:        types.OperationCreateSymlink,
					Source:      "/dotfiles/bin",
					Target:      filepath.Join(paths.GetPathDir(), "tools"),
					Description: "Add tools to PATH",
				},
			},
		},
		{
			name: "path_add_action_no_pack",
			action: types.Action{
				Type:   types.ActionTypePathAdd,
				Source: "/usr/local/mybin",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      paths.GetPathDir(),
					Description: "Create PATH deployment directory",
				},
				{
					Type:        types.OperationCreateSymlink,
					Source:      "/usr/local/mybin",
					Target:      filepath.Join(paths.GetPathDir(), "mybin"),
					Description: "Add mybin to PATH",
				},
			},
		},
		{
			name: "path_add_action_missing_source",
			action: types.Action{
				Type: types.ActionTypePathAdd,
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		// Run action tests
		{
			name: "run_action_returns_nil",
			action: types.Action{
				Type:    types.ActionTypeRun,
				Command: "install.sh",
			},
			wantOps: nil,
		},
		// Unknown action type
		{
			name: "unknown_action_type",
			action: types.Action{
				Type: "unknown",
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		// Brew action tests
		{
			name: "brew_action_success",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "/packs/tools/Brewfile",
				Metadata: map[string]interface{}{
					"checksum": "abc123def456",
					"pack":     "tools",
				},
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      paths.GetBrewfileDir(),
					Description: "Create brewfile sentinel directory",
				},
				{
					Type:        types.OperationWriteFile,
					Target:      filepath.Join(paths.GetBrewfileDir(), "tools"),
					Content:     "abc123def456",
					Mode:        uint32Ptr(0644),
					Description: "Create brewfile sentinel for tools",
				},
			},
		},
		{
			name: "brew_action_missing_source",
			action: types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"checksum": "abc123",
					"pack":     "tools",
				},
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		{
			name: "brew_action_missing_checksum",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "/packs/tools/Brewfile",
				Metadata: map[string]interface{}{
					"pack": "tools",
				},
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		{
			name: "brew_action_missing_pack",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "/packs/tools/Brewfile",
				Metadata: map[string]interface{}{
					"checksum": "abc123",
				},
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		// Install action tests
		{
			name: "install_action_success",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "/packs/dev/install.sh",
				Metadata: map[string]interface{}{
					"checksum": "def789ghi012",
					"pack":     "dev",
				},
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      paths.GetInstallDir(),
					Description: "Create install sentinel directory",
				},
				{
					Type:        types.OperationWriteFile,
					Target:      filepath.Join(paths.GetInstallDir(), "dev"),
					Content:     "def789ghi012",
					Mode:        uint32Ptr(0644),
					Description: "Create install sentinel for dev",
				},
			},
		},
		{
			name: "install_action_missing_source",
			action: types.Action{
				Type: types.ActionTypeInstall,
				Metadata: map[string]interface{}{
					"checksum": "abc123",
					"pack":     "dev",
				},
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		{
			name: "install_action_missing_checksum",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "/packs/dev/install.sh",
				Metadata: map[string]interface{}{
					"pack": "dev",
				},
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		{
			name: "install_action_missing_pack",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "/packs/dev/install.sh",
				Metadata: map[string]interface{}{
					"checksum": "abc123",
				},
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		{
			name: "read_action",
			action: types.Action{
				Type:   types.ActionTypeRead,
				Source: "/packs/vim/.vimrc",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationReadFile,
					Source:      "/packs/vim/.vimrc",
					Description: "Read file .vimrc",
				},
			},
		},
		{
			name: "read_action_missing_source",
			action: types.Action{
				Type: types.ActionTypeRead,
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
		{
			name: "checksum_action",
			action: types.Action{
				Type:   types.ActionTypeChecksum,
				Source: "/packs/tools/Brewfile",
			},
			wantOps: []types.Operation{
				{
					Type:        types.OperationChecksum,
					Source:      "/packs/tools/Brewfile",
					Description: "Calculate checksum for Brewfile",
				},
			},
		},
		{
			name: "checksum_action_missing_source",
			action: types.Action{
				Type: types.ActionTypeChecksum,
			},
			wantError: true,
			errorCode: doerrors.ErrActionInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ops, err := ConvertAction(tt.action)

			if tt.wantError {
				testutil.AssertError(t, err)
				if tt.errorCode != "" {
					var dodotErr *doerrors.DodotError
					testutil.AssertTrue(t, errors.As(err, &dodotErr),
						"Expected DodotError but got %T", err)
					testutil.AssertEqual(t, tt.errorCode, dodotErr.Code)
				}
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, len(tt.wantOps), len(ops),
				"Expected %d operations, got %d", len(tt.wantOps), len(ops))

			for i, wantOp := range tt.wantOps {
				if i >= len(ops) {
					break
				}
				gotOp := ops[i]
				testutil.AssertEqual(t, wantOp.Type, gotOp.Type)
				testutil.AssertEqual(t, wantOp.Source, gotOp.Source)
				testutil.AssertEqual(t, wantOp.Target, gotOp.Target)
				testutil.AssertEqual(t, wantOp.Content, gotOp.Content)
				testutil.AssertEqual(t, wantOp.Description, gotOp.Description)

				// Compare Mode pointers
				if wantOp.Mode == nil && gotOp.Mode == nil {
					// Both nil, ok
				} else if wantOp.Mode != nil && gotOp.Mode != nil {
					testutil.AssertEqual(t, *wantOp.Mode, *gotOp.Mode)
				} else {
					// One is nil, one is not
					t.Errorf("Mode mismatch: want %v, got %v", wantOp.Mode, gotOp.Mode)
				}
			}
		})
	}
}

func TestExpandHome(t *testing.T) {
	homeDir, _ := os.UserHomeDir()

	tests := []struct {
		name     string
		path     string
		expected string
	}{
		{
			name:     "tilde_only",
			path:     "~",
			expected: homeDir,
		},
		{
			name:     "tilde_with_path",
			path:     "~/Documents/file.txt",
			expected: filepath.Join(homeDir, "Documents/file.txt"),
		},
		{
			name:     "no_tilde",
			path:     "/absolute/path",
			expected: "/absolute/path",
		},
		{
			name:     "tilde_not_at_start",
			path:     "/path/~/file",
			expected: "/path/~/file",
		},
		{
			name:     "relative_path",
			path:     "relative/path",
			expected: "relative/path",
		},
		{
			name:     "empty_path",
			path:     "",
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := expandHome(tt.path)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

// Benchmarks
func BenchmarkGetFileOperations(b *testing.B) {
	actions := []types.Action{
		{Type: types.ActionTypeLink, Source: "/src1", Target: "~/dst1"},
		{Type: types.ActionTypeCopy, Source: "/src2", Target: "~/dst2"},
		{Type: types.ActionTypeWrite, Target: "~/file", Content: "data"},
		{Type: types.ActionTypeMkdir, Target: "~/dir"},
		{Type: types.ActionTypeShellSource, Source: "/script.sh"},
		{Type: types.ActionTypePathAdd, Source: "/bin"},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := GetFileOperations(actions)
		if err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkConvertAction_Link(b *testing.B) {
	action := types.Action{
		Type:   types.ActionTypeLink,
		Source: "/source/file",
		Target: "~/.config/app/file",
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := ConvertAction(action)
		if err != nil {
			b.Fatal(err)
		}
	}
}

// TestNoDuplicateDirectoryOperations tests that we don't generate duplicate
// directory creation operations when multiple files target the same parent directory
func TestNoDuplicateDirectoryOperations(t *testing.T) {
	// Create actions that will create files in the same directory
	actions := []types.Action{
		{
			Type:        types.ActionTypeLink,
			Source:      "/dotfiles/vim/.vimrc",
			Target:      "~/.vimrc",
			Description: "Symlink .vimrc",
		},
		{
			Type:        types.ActionTypeLink,
			Source:      "/dotfiles/bash/.bashrc",
			Target:      "~/.bashrc",
			Description: "Symlink .bashrc",
		},
		{
			Type:        types.ActionTypeLink,
			Source:      "/dotfiles/zsh/.zshrc",
			Target:      "~/.zshrc",
			Description: "Symlink .zshrc",
		},
	}

	// Convert to operations
	ops, err := GetFileOperations(actions)
	require.NoError(t, err)

	// Count directory creation operations for the home directory
	homeDir := expandHome("~")
	dirOpCount := 0
	for _, op := range ops {
		if op.Type == types.OperationCreateDir && op.Target == homeDir {
			dirOpCount++
		}
	}

	// We should only have ONE directory creation operation for the home directory
	assert.Equal(t, 1, dirOpCount, "Expected exactly one directory creation operation for home directory, got %d", dirOpCount)

	// Verify we have all the symlink operations (3 files × 2 symlinks each = 6)
	symlinkCount := 0
	for _, op := range ops {
		if op.Type == types.OperationCreateSymlink {
			symlinkCount++
		}
	}
	assert.Equal(t, 6, symlinkCount, "Expected 6 symlink operations (2 per file)")
}

// TestExecutionPipelineNoDuplicateOperations tests that the execution pipeline
// doesn't create duplicate operations when running with checksums
func TestExecutionPipelineNoDuplicateOperations(t *testing.T) {
	// Create actions that include both regular and checksum operations
	actions := []types.Action{
		{
			Type:        types.ActionTypeLink,
			Source:      "/dotfiles/vim/.vimrc",
			Target:      "~/.vimrc",
			Description: "Symlink .vimrc",
		},
		{
			Type:        types.ActionTypeChecksum,
			Source:      "/dotfiles/brew/Brewfile",
			Description: "Checksum Brewfile",
		},
		{
			Type:        types.ActionTypeBrew,
			Source:      "/dotfiles/brew/Brewfile",
			Target:      "~/.local/share/dodot/brewfile/Brewfile",
			Description: "Install from Brewfile",
			Metadata: map[string]interface{}{
				"checksum_source": "/dotfiles/brew/Brewfile",
				"checksum": "abc123", // Provide checksum to avoid validation error
				"pack": "brew",
			},
		},
	}

	// Create context with checksum result
	ctx := NewExecutionContext()
	ctx.ChecksumResults["/dotfiles/brew/Brewfile"] = "abc123"

	// Generate operations with context (this is what the pipeline does)
	finalOps, err := GetFileOperationsWithContext(actions, ctx)
	require.NoError(t, err)

	// Count operations by type and target
	opCounts := make(map[string]int)
	for _, op := range finalOps {
		key := string(op.Type) + ":" + op.Target
		opCounts[key]++
	}

	// Verify no duplicates
	for key, count := range opCounts {
		assert.Equal(t, 1, count, "Operation %s should appear exactly once, but appeared %d times", key, count)
	}
}

// TestDuplicateOperationsDifferentDescriptions tests that operations with the same
// type and target but different descriptions are still considered duplicates
func TestDuplicateOperationsDifferentDescriptions(t *testing.T) {
	ops := []types.Operation{
		{
			Type:        types.OperationCreateDir,
			Target:      "/home/user",
			Description: "Create parent directory for .vimrc",
		},
		{
			Type:        types.OperationCreateDir,
			Target:      "/home/user",
			Description: "Create parent directory for .bashrc",
		},
	}

	// This should be deduplicated to just one operation
	deduped := deduplicateOperations(ops)
	assert.Equal(t, 1, len(deduped), "Expected duplicate directory operations to be deduplicated")
	
	// The first operation should be kept
	assert.Equal(t, "Create parent directory for .vimrc", deduped[0].Description)
}

// TestDeduplicateOperationsPreservesOrder tests that deduplication preserves
// the order of operations and keeps the first occurrence
func TestDeduplicateOperationsPreservesOrder(t *testing.T) {
	homeDir := expandHome("~")
	deployedDir := filepath.Join(homeDir, ".local", "share", "dodot", "deployed", "symlink")
	
	ops := []types.Operation{
		{
			Type:        types.OperationCreateDir,
			Target:      homeDir,
			Description: "Create home directory",
		},
		{
			Type:        types.OperationCreateDir,
			Target:      deployedDir,
			Description: "Create deployed directory",
		},
		{
			Type:        types.OperationCreateSymlink,
			Source:      "/dotfiles/vim/.vimrc",
			Target:      filepath.Join(deployedDir, ".vimrc"),
			Description: "Deploy .vimrc",
		},
		{
			Type:        types.OperationCreateDir,
			Target:      homeDir,
			Description: "Create home directory again",
		},
		{
			Type:        types.OperationCreateSymlink,
			Source:      filepath.Join(deployedDir, ".vimrc"),
			Target:      filepath.Join(homeDir, ".vimrc"),
			Description: "Symlink .vimrc",
		},
	}

	deduped := deduplicateOperations(ops)
	
	// Should have 4 operations (one duplicate removed)
	assert.Equal(t, 4, len(deduped))
	
	// Order should be preserved
	assert.Equal(t, types.OperationCreateDir, deduped[0].Type)
	assert.Equal(t, homeDir, deduped[0].Target)
	assert.Equal(t, "Create home directory", deduped[0].Description) // First occurrence kept
	
	assert.Equal(t, types.OperationCreateDir, deduped[1].Type)
	assert.Equal(t, deployedDir, deduped[1].Target)
	
	assert.Equal(t, types.OperationCreateSymlink, deduped[2].Type)
	assert.Equal(t, types.OperationCreateSymlink, deduped[3].Type)
}
