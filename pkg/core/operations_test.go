package core

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	doerrors "github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestConvertActionsToOperations(t *testing.T) {
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
					Source:      "{{DOTFILES_ROOT}}/app/config.yml",
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
				testutil.AssertContains(t, ops[1].Source, "/app/config.yml")

				// User symlink
				testutil.AssertEqual(t, types.OperationCreateSymlink, ops[2].Type)
			},
		},
		{
			name: "multiple_actions_sorted_by_priority",
			actions: []types.Action{
				{
					Type:     types.ActionTypeLink,
					Source:   "{{DOTFILES_ROOT}}/source/low",
					Target:   "~/low",
					Priority: 10,
				},
				{
					Type:     types.ActionTypeLink,
					Source:   "{{DOTFILES_ROOT}}/source/high",
					Target:   "~/high",
					Priority: 100,
				},
				{
					Type:     types.ActionTypeLink,
					Source:   "{{DOTFILES_ROOT}}/source/medium",
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
				testutil.AssertContains(t, deployOps[0].Source, "/source/high")
				// Medium priority second
				testutil.AssertContains(t, deployOps[1].Source, "/source/medium")
				// Low priority last
				testutil.AssertContains(t, deployOps[2].Source, "/source/low")
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
			name: "brew_and_install_actions_without_context",
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
			wantOpsCount: 5, // With context: 2 create dir + 2 write file (sentinels) + 1 execute (install only)
			wantError:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create context for tests that have link actions
			var ops []types.Operation
			var err error

			testPaths := createTestPaths(t)
			ctx := NewExecutionContextWithHomeSymlinks(false, testPaths, true, nil)

			// Replace {{DOTFILES_ROOT}} placeholder in source paths
			for i := range tt.actions {
				if tt.actions[i].Source != "" {
					tt.actions[i].Source = strings.ReplaceAll(tt.actions[i].Source, "{{DOTFILES_ROOT}}", testPaths.DotfilesRoot())
				}
			}

			ops, err = ConvertActionsToOperationsWithContext(tt.actions, ctx)

			if tt.wantError {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)

			// Debug: print operations for brew/install test
			if tt.name == "brew_and_install_actions_without_context" {
				t.Logf("Generated %d operations:", len(ops))
				for i, op := range ops {
					t.Logf("  %d: %s - %s", i, op.Type, op.Description)
				}
			}

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
		wantOps   func(*paths.Paths) []types.Operation
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
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      homeDir,
						Description: "Create parent directory for .vimrc",
					},
					{
						Type:        types.OperationCreateSymlink,
						Source:      filepath.Join(p.DotfilesRoot(), ".vimrc"), // Updated to match transformed path
						Target:      filepath.Join(p.SymlinkDir(), ".vimrc"),
						Description: "Deploy symlink for .vimrc",
					},
					{
						Type:        types.OperationCreateSymlink,
						Source:      filepath.Join(p.SymlinkDir(), ".vimrc"),
						Target:      filepath.Join(homeDir, ".vimrc"),
						Description: "Link vimrc",
					},
				}
			},
		},
		{
			name: "link_action_with_parent_dir",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "/source/config.yml",
				Target: "~/.config/app/config.yml",
			},
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      filepath.Join(homeDir, ".config/app"),
						Description: "Create parent directory for config.yml",
					},
					{
						Type:        types.OperationCreateSymlink,
						Source:      filepath.Join(p.DotfilesRoot(), "config.yml"), // Updated to match transformed path
						Target:      filepath.Join(p.SymlinkDir(), "config.yml"),
						Description: "Deploy symlink for config.yml",
					},
					{
						Type:        types.OperationCreateSymlink,
						Source:      filepath.Join(p.SymlinkDir(), "config.yml"),
						Target:      filepath.Join(homeDir, ".config/app/config.yml"),
						Description: "",
					},
				}
			},
		},
		{
			name: "link_action_missing_source",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "",
				Target: "~/target",
			},
			wantOps:   nil,
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
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      homeDir,
						Description: "Create parent directory for .gitconfig",
					},
					{
						Type:        types.OperationCopyFile,
						Source:      filepath.Join(p.DotfilesRoot(), "gitconfig"), // Updated to match transformed path
						Target:      filepath.Join(homeDir, ".gitconfig"),
						Description: "Copy template",
					},
				}
			},
		},
		{
			name: "copy_action_with_parent_dir",
			action: types.Action{
				Type:   types.ActionTypeCopy,
				Source: "/source/data.json",
				Target: "~/.config/app/data.json",
			},
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      filepath.Join(homeDir, ".config/app"),
						Description: "Create parent directory for data.json",
					},
					{
						Type:        types.OperationCopyFile,
						Source:      filepath.Join(p.DotfilesRoot(), "data.json"), // Updated to match transformed path
						Target:      filepath.Join(homeDir, ".config/app/data.json"),
						Description: "",
					},
				}
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
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      homeDir,
						Description: "Create parent directory for .myapp.conf",
					},
					{
						Type:        types.OperationWriteFile,
						Target:      filepath.Join(homeDir, ".myapp.conf"),
						Content:     "# My App Config\nkey=value",
						Mode:        operations.Uint32Ptr(0644),
						Description: "Create config",
					},
				}
			},
		},
		{
			name: "write_action_no_mode",
			action: types.Action{
				Type:    types.ActionTypeWrite,
				Target:  "~/file.txt",
				Content: "content",
			},
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
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
				}
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
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationWriteFile,
						Target:      filepath.Join(homeDir, ".bashrc"),
						Content:     "\n# Added by dodot\nexport FOO=bar",
						Description: "Append to ~/.bashrc",
					},
				}
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
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      filepath.Join(homeDir, ".config/myapp"),
						Mode:        operations.Uint32Ptr(0755),
						Description: "Create app dir",
					},
				}
			},
		},
		{
			name: "mkdir_action_no_mode",
			action: types.Action{
				Type:   types.ActionTypeMkdir,
				Target: "~/somedir",
			},
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      filepath.Join(homeDir, "somedir"),
						Mode:        nil,
						Description: "",
					},
				}
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
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      p.ShellProfileDir(),
						Description: "Create shell profile deployment directory",
					},
					{
						Type:        types.OperationCreateSymlink,
						Source:      filepath.Join(p.DotfilesRoot(), "aliases.sh"), // Updated to match transformed path
						Target:      filepath.Join(p.ShellProfileDir(), "shell.sh"),
						Description: "Deploy shell profile script from shell",
					},
				}
			},
		},
		{
			name: "shell_source_action_no_pack",
			action: types.Action{
				Type:   types.ActionTypeShellSource,
				Source: "/dotfiles/custom.sh",
			},
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      p.ShellProfileDir(),
						Description: "Create shell profile deployment directory",
					},
					{
						Type:        types.OperationCreateSymlink,
						Source:      filepath.Join(p.DotfilesRoot(), "custom.sh"), // Updated to match transformed path
						Target:      filepath.Join(p.ShellProfileDir(), "custom.sh"),
						Description: "Deploy shell profile script from ",
					},
				}
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
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      p.PathDir(),
						Description: "Create PATH deployment directory",
					},
					{
						Type:        types.OperationCreateSymlink,
						Source:      filepath.Join(p.DotfilesRoot(), "bin"), // Updated to match transformed path
						Target:      filepath.Join(p.PathDir(), "tools"),
						Description: "Add tools to PATH",
					},
				}
			},
		},
		{
			name: "path_add_action_no_pack",
			action: types.Action{
				Type:   types.ActionTypePathAdd,
				Source: "/usr/local/mybin",
			},
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationCreateDir,
						Target:      p.PathDir(),
						Description: "Create PATH deployment directory",
					},
					{
						Type:        types.OperationCreateSymlink,
						Source:      filepath.Join(p.DotfilesRoot(), "mybin"), // Updated to match transformed path
						Target:      filepath.Join(p.PathDir(), "mybin"),
						Description: "Add mybin to PATH",
					},
				}
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
			name: "brew_action_without_context",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "/packs/tools/Brewfile",
				Metadata: map[string]interface{}{
					"checksum": "abc123def456",
					"pack":     "tools",
				},
			},
			wantOps: nil, // Without context, brew actions are skipped
		},
		{
			name: "brew_action_missing_source_without_context",
			action: types.Action{
				Type: types.ActionTypeBrew,
				Metadata: map[string]interface{}{
					"checksum": "abc123",
					"pack":     "tools",
				},
			},
			wantOps: nil, // Without context, skipped before validation
		},
		{
			name: "brew_action_missing_checksum_without_context",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "/packs/tools/Brewfile",
				Metadata: map[string]interface{}{
					"pack": "tools",
				},
			},
			wantOps: nil, // Without context, skipped before validation
		},
		{
			name: "brew_action_missing_pack_without_context",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "/packs/tools/Brewfile",
				Metadata: map[string]interface{}{
					"checksum": "abc123",
				},
			},
			wantOps: nil, // Without context, skipped before validation
		},
		// Install action tests
		{
			name: "install_action_without_context",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "/packs/dev/install.sh",
				Metadata: map[string]interface{}{
					"checksum": "def789ghi012",
					"pack":     "dev",
				},
			},
			wantOps: nil, // Without context, install actions are skipped
		},
		{
			name: "install_action_missing_source_without_context",
			action: types.Action{
				Type: types.ActionTypeInstall,
				Metadata: map[string]interface{}{
					"checksum": "abc123",
					"pack":     "dev",
				},
			},
			wantOps: nil, // Without context, skipped before validation
		},
		{
			name: "install_action_missing_checksum_without_context",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "/packs/dev/install.sh",
				Metadata: map[string]interface{}{
					"pack": "dev",
				},
			},
			wantOps: nil, // Without context, skipped before validation
		},
		{
			name: "install_action_missing_pack_without_context",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "/packs/dev/install.sh",
				Metadata: map[string]interface{}{
					"checksum": "abc123",
				},
			},
			wantOps: nil, // Without context, skipped before validation
		},
		{
			name: "read_action",
			action: types.Action{
				Type:   types.ActionTypeRead,
				Source: "/packs/vim/.vimrc",
			},
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationReadFile,
						Source:      filepath.Join(p.DotfilesRoot(), ".vimrc"), // Updated to match transformed path
						Description: "Read file .vimrc",
					},
				}
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
			wantOps: func(p *paths.Paths) []types.Operation {
				return []types.Operation{
					{
						Type:        types.OperationChecksum,
						Source:      filepath.Join(p.DotfilesRoot(), "Brewfile"), // Updated to match transformed path
						Description: "Calculate checksum for Brewfile",
					},
				}
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
			var ops []types.Operation
			var err error
			var testPaths *paths.Paths

			// For actions that need paths, create testPaths
			needsPaths := tt.wantOps != nil ||
				tt.action.Type == types.ActionTypeLink ||
				tt.action.Type == types.ActionTypeShellSource ||
				tt.action.Type == types.ActionTypePathAdd ||
				tt.action.Type == types.ActionTypeCopy ||
				tt.action.Type == types.ActionTypeWrite ||
				tt.action.Type == types.ActionTypeAppend ||
				tt.action.Type == types.ActionTypeMkdir ||
				tt.action.Type == types.ActionTypeRead ||
				tt.action.Type == types.ActionTypeChecksum

			if needsPaths {
				testPaths = createTestPaths(t)
				// Replace placeholder in source if present
				if tt.action.Source != "" && strings.Contains(tt.action.Source, "{{DOTFILES_ROOT}}") {
					tt.action.Source = strings.ReplaceAll(tt.action.Source, "{{DOTFILES_ROOT}}", testPaths.DotfilesRoot())
				} else if tt.action.Source != "" && strings.HasPrefix(tt.action.Source, "/") {
					// If it's an absolute path not in dotfiles, move it to dotfiles root
					tt.action.Source = filepath.Join(testPaths.DotfilesRoot(), filepath.Base(tt.action.Source))
				}
				ctx := NewExecutionContextWithHomeSymlinks(false, testPaths, true, nil)
				ops, err = ConvertActionWithContext(tt.action, ctx)
			} else {
				// For other actions (brew, install), use nil context to test skipping behavior
				ops, err = ConvertActionWithContext(tt.action, nil)
			}

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

			// Generate expected operations if function is provided
			var expectedOps []types.Operation
			if tt.wantOps != nil && testPaths != nil {
				expectedOps = tt.wantOps(testPaths)
			}

			testutil.AssertEqual(t, len(expectedOps), len(ops),
				"Expected %d operations, got %d", len(expectedOps), len(ops))

			for i, wantOp := range expectedOps {
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
			result := operations.ExpandHome(tt.path)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

// Benchmarks
func BenchmarkConvertActionsToOperations(b *testing.B) {
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
		_, err := ConvertActionsToOperations(actions)
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
	testPaths := createTestPaths(t)
	// Replace source paths with test dotfiles root
	for i := range actions {
		if actions[i].Source != "" {
			actions[i].Source = filepath.Join(testPaths.DotfilesRoot(), filepath.Base(actions[i].Source))
		}
	}
	ctx := NewExecutionContextWithHomeSymlinks(false, testPaths, true, nil)
	ops, err := ConvertActionsToOperationsWithContext(actions, ctx)
	require.NoError(t, err)

	// Count directory creation operations for the home directory
	homeDir := operations.ExpandHome("~")
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

// TestConvertActionsToOperationsWithContext_BrewAndInstall tests brew and install actions with context
func TestConvertActionsToOperationsWithContext_BrewAndInstall(t *testing.T) {
	// Create context with checksums
	testPaths := createTestPaths(t)
	ctx := NewExecutionContext(false, testPaths)
	ctx.ChecksumResults["/packs/tools/Brewfile"] = "brew123"
	ctx.ChecksumResults["/packs/dev/install.sh"] = "install456"

	actions := []types.Action{
		{
			Type:     types.ActionTypeBrew,
			Source:   "/packs/tools/Brewfile",
			Priority: 10,
			Metadata: map[string]interface{}{
				"pack": "tools",
			},
		},
		{
			Type:     types.ActionTypeInstall,
			Source:   "/packs/dev/install.sh",
			Priority: 20,
			Metadata: map[string]interface{}{
				"pack": "dev",
			},
		},
	}

	ops, err := ConvertActionsToOperationsWithContext(actions, ctx)
	require.NoError(t, err)
	assert.Len(t, ops, 5) // After deduplication: 2 create dir ops + 2 execute ops + 2 write sentinel ops - 1 duplicate dir

	// Install action should be processed first (higher priority)
	// First: create install directory
	assert.Equal(t, types.OperationCreateDir, ops[0].Type)
	assert.Equal(t, testPaths.InstallDir(), ops[0].Target)

	// Second: execute install script
	assert.Equal(t, types.OperationExecute, ops[1].Type)
	assert.Equal(t, "/bin/sh", ops[1].Command)

	// Third: write install sentinel
	assert.Equal(t, types.OperationWriteFile, ops[2].Type)
	assert.Contains(t, ops[2].Target, "dev")
	assert.Equal(t, "install456", ops[2].Content)

	// Then brew action
	// Fourth: create brewfile directory
	assert.Equal(t, types.OperationCreateDir, ops[3].Type)
	assert.Equal(t, testPaths.HomebrewDir(), ops[3].Target)

	// Fifth: write brewfile sentinel (execute was deduplicated)
	assert.Equal(t, types.OperationWriteFile, ops[4].Type)
	assert.Contains(t, ops[4].Target, "tools")
	assert.Equal(t, "brew123", ops[4].Content)
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
				"checksum":        "abc123", // Provide checksum to avoid validation error
				"pack":            "brew",
			},
		},
	}

	// Create context with checksum result
	testPaths := createTestPaths(t)

	// Replace source paths with test dotfiles root
	for i := range actions {
		if actions[i].Source != "" {
			actions[i].Source = filepath.Join(testPaths.DotfilesRoot(), filepath.Base(actions[i].Source))
		}
	}

	ctx := NewExecutionContextWithHomeSymlinks(false, testPaths, true, nil)
	// Update checksum result path to match transformed source
	ctx.ChecksumResults[filepath.Join(testPaths.DotfilesRoot(), "Brewfile")] = "abc123"

	// Generate operations with context (this is what the pipeline does)
	finalOps, err := ConvertActionsToOperationsWithContext(actions, ctx)
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
	deduped := operations.DeduplicateOperations(ops)
	assert.Equal(t, 1, len(deduped), "Expected duplicate directory operations to be deduplicated")

	// The first operation should be kept
	assert.Equal(t, "Create parent directory for .vimrc", deduped[0].Description)
}

// TestDeduplicateOperationsPreservesOrder tests that deduplication preserves
// the order of operations and keeps the first occurrence
func TestDeduplicateOperationsPreservesOrder(t *testing.T) {
	homeDir := operations.ExpandHome("~")
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

	deduped := operations.DeduplicateOperations(ops)

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
