package types

import (
	"errors"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewExecutionContext(t *testing.T) {
	tests := []struct {
		name    string
		command string
		dryRun  bool
	}{
		{
			name:    "deploy command dry run",
			command: "link",
			dryRun:  true,
		},
		{
			name:    "install command real run",
			command: "install",
			dryRun:  false,
		},
		{
			name:    "status command",
			command: "status",
			dryRun:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ec := NewExecutionContext(tt.command, tt.dryRun)

			assert.Equal(t, tt.command, ec.Command)
			assert.Equal(t, tt.dryRun, ec.DryRun)
			assert.NotNil(t, ec.PackResults)
			assert.Empty(t, ec.PackResults)
			assert.False(t, ec.StartTime.IsZero())
			assert.True(t, ec.EndTime.IsZero())
			assert.Equal(t, 0, ec.TotalActions)
			assert.Equal(t, 0, ec.CompletedActions)
			assert.Equal(t, 0, ec.FailedActions)
			assert.Equal(t, 0, ec.SkippedActions)
		})
	}
}

func TestExecutionContext_AddPackResult(t *testing.T) {
	ec := NewExecutionContext("link", false)

	// Create pack results with different statuses
	pack1Result := &PackExecutionResult{
		Pack:              &Pack{Name: "vim"},
		TotalHandlers:     5,
		CompletedHandlers: 3,
		FailedHandlers:    1,
		SkippedHandlers:   1,
	}

	pack2Result := &PackExecutionResult{
		Pack:              &Pack{Name: "zsh"},
		TotalHandlers:     3,
		CompletedHandlers: 2,
		FailedHandlers:    0,
		SkippedHandlers:   1,
	}

	// Add first pack
	ec.AddPackResult("vim", pack1Result)
	assert.Equal(t, 1, len(ec.PackResults))
	assert.Equal(t, 5, ec.TotalActions)
	assert.Equal(t, 3, ec.CompletedActions)
	assert.Equal(t, 1, ec.FailedActions)
	assert.Equal(t, 1, ec.SkippedActions)

	// Add second pack
	ec.AddPackResult("zsh", pack2Result)
	assert.Equal(t, 2, len(ec.PackResults))
	assert.Equal(t, 8, ec.TotalActions)
	assert.Equal(t, 5, ec.CompletedActions)
	assert.Equal(t, 1, ec.FailedActions)
	assert.Equal(t, 2, ec.SkippedActions)

	// Update first pack (should recalculate totals)
	pack1Result.CompletedHandlers = 4
	pack1Result.FailedHandlers = 0
	ec.AddPackResult("vim", pack1Result)
	assert.Equal(t, 2, len(ec.PackResults))
	assert.Equal(t, 8, ec.TotalActions)
	assert.Equal(t, 6, ec.CompletedActions)
	assert.Equal(t, 0, ec.FailedActions)
	assert.Equal(t, 2, ec.SkippedActions)
}

func TestExecutionContext_GetPackResult(t *testing.T) {
	ec := NewExecutionContext("link", false)

	packResult := &PackExecutionResult{
		Pack:          &Pack{Name: "vim"},
		TotalHandlers: 5,
	}

	// Test getting non-existent pack
	result, ok := ec.GetPackResult("vim")
	assert.Nil(t, result)
	assert.False(t, ok)

	// Add pack and retrieve it
	ec.AddPackResult("vim", packResult)
	result, ok = ec.GetPackResult("vim")
	assert.NotNil(t, result)
	assert.True(t, ok)
	assert.Equal(t, packResult, result)

	// Test getting different non-existent pack
	result, ok = ec.GetPackResult("zsh")
	assert.Nil(t, result)
	assert.False(t, ok)
}

func TestExecutionContext_Complete(t *testing.T) {
	ec := NewExecutionContext("link", false)

	// Initially EndTime should be zero
	assert.True(t, ec.EndTime.IsZero())

	// Complete the execution
	ec.Complete()

	// EndTime should be set
	assert.False(t, ec.EndTime.IsZero())
	assert.True(t, ec.EndTime.After(ec.StartTime))
}

func TestNewPackExecutionResult(t *testing.T) {
	pack := &Pack{
		Name: "vim",
		Path: "/home/user/.dotfiles/vim",
	}

	per := NewPackExecutionResult(pack)

	assert.Equal(t, pack, per.Pack)
	assert.NotNil(t, per.HandlerResults)
	assert.Empty(t, per.HandlerResults)
	assert.Equal(t, ExecutionStatusPending, per.Status)
	assert.False(t, per.StartTime.IsZero())
	assert.True(t, per.EndTime.IsZero())
	assert.Equal(t, 0, per.TotalHandlers)
	assert.Equal(t, 0, per.CompletedHandlers)
	assert.Equal(t, 0, per.FailedHandlers)
	assert.Equal(t, 0, per.SkippedHandlers)
}

func TestPackExecutionResult_AddHandlerResult(t *testing.T) {
	pack := &Pack{Name: "vim"}

	tests := []struct {
		name              string
		results           []*HandlerResult
		expectedTotal     int
		expectedCompleted int
		expectedFailed    int
		expectedSkipped   int
		expectedStatus    ExecutionStatus
	}{
		{
			name: "single success",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusReady},
			},
			expectedTotal:     1,
			expectedCompleted: 1,
			expectedFailed:    0,
			expectedSkipped:   0,
			expectedStatus:    ExecutionStatusSuccess,
		},
		{
			name: "single error",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusError},
			},
			expectedTotal:     1,
			expectedCompleted: 0,
			expectedFailed:    1,
			expectedSkipped:   0,
			expectedStatus:    ExecutionStatusError,
		},
		{
			name: "single conflict",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusConflict},
			},
			expectedTotal:     1,
			expectedCompleted: 0,
			expectedFailed:    1,
			expectedSkipped:   0,
			expectedStatus:    ExecutionStatusError,
		},
		{
			name: "single skipped",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusSkipped},
			},
			expectedTotal:     1,
			expectedCompleted: 0,
			expectedFailed:    0,
			expectedSkipped:   1,
			expectedStatus:    ExecutionStatusSkipped,
		},
		{
			name: "mixed success and error",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusReady},
				{HandlerName: "homebrew", Status: StatusError},
			},
			expectedTotal:     2,
			expectedCompleted: 1,
			expectedFailed:    1,
			expectedSkipped:   0,
			expectedStatus:    ExecutionStatusPartial,
		},
		{
			name: "all success",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusReady},
				{HandlerName: "homebrew", Status: StatusReady},
				{HandlerName: "shell", Status: StatusReady},
			},
			expectedTotal:     3,
			expectedCompleted: 3,
			expectedFailed:    0,
			expectedSkipped:   0,
			expectedStatus:    ExecutionStatusSuccess,
		},
		{
			name: "all error",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusError},
				{HandlerName: "homebrew", Status: StatusConflict},
			},
			expectedTotal:     2,
			expectedCompleted: 0,
			expectedFailed:    2,
			expectedSkipped:   0,
			expectedStatus:    ExecutionStatusError,
		},
		{
			name: "all skipped",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusSkipped},
				{HandlerName: "homebrew", Status: StatusSkipped},
			},
			expectedTotal:     2,
			expectedCompleted: 0,
			expectedFailed:    0,
			expectedSkipped:   2,
			expectedStatus:    ExecutionStatusSkipped,
		},
		{
			name: "complex mix",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusReady},
				{HandlerName: "homebrew", Status: StatusError},
				{HandlerName: "shell", Status: StatusSkipped},
				{HandlerName: "path", Status: StatusReady},
				{HandlerName: "install", Status: StatusConflict},
			},
			expectedTotal:     5,
			expectedCompleted: 2,
			expectedFailed:    2,
			expectedSkipped:   1,
			expectedStatus:    ExecutionStatusPartial,
		},
		{
			name: "unknown status treated as none",
			results: []*HandlerResult{
				{HandlerName: "symlink", Status: StatusUnknown},
			},
			expectedTotal:     1,
			expectedCompleted: 0,
			expectedFailed:    0,
			expectedSkipped:   0,
			expectedStatus:    ExecutionStatusSuccess,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			per := NewPackExecutionResult(pack)

			for _, result := range tt.results {
				per.AddHandlerResult(result)
			}

			assert.Equal(t, tt.expectedTotal, per.TotalHandlers)
			assert.Equal(t, tt.expectedCompleted, per.CompletedHandlers)
			assert.Equal(t, tt.expectedFailed, per.FailedHandlers)
			assert.Equal(t, tt.expectedSkipped, per.SkippedHandlers)
			assert.Equal(t, tt.expectedStatus, per.Status)
			assert.Equal(t, len(tt.results), len(per.HandlerResults))
		})
	}
}

func TestPackExecutionResult_updateStatus(t *testing.T) {
	// This is tested through AddHandlerResult, but let's test edge cases
	pack := &Pack{Name: "vim"}
	per := NewPackExecutionResult(pack)

	// Empty pack should be pending
	per.updateStatus()
	assert.Equal(t, ExecutionStatusPending, per.Status)

	// Manually set counts to test edge cases
	per.TotalHandlers = 3
	per.CompletedHandlers = 3
	per.updateStatus()
	assert.Equal(t, ExecutionStatusSuccess, per.Status)
}

func TestPackExecutionResult_Complete(t *testing.T) {
	pack := &Pack{Name: "vim"}
	per := NewPackExecutionResult(pack)

	// Add some results
	per.AddHandlerResult(&HandlerResult{Status: StatusReady})
	per.AddHandlerResult(&HandlerResult{Status: StatusError})

	// Complete should set EndTime and update status
	assert.True(t, per.EndTime.IsZero())
	per.Complete()
	assert.False(t, per.EndTime.IsZero())
	assert.Equal(t, ExecutionStatusPartial, per.Status)
}

func TestMapOperationStatusToDisplayStatus(t *testing.T) {
	tests := []struct {
		name     string
		status   OperationStatus
		expected string
	}{
		{
			name:     "ready maps to success",
			status:   StatusReady,
			expected: "success",
		},
		{
			name:     "error maps to error",
			status:   StatusError,
			expected: "error",
		},
		{
			name:     "skipped maps to queue",
			status:   StatusSkipped,
			expected: "queue",
		},
		{
			name:     "conflict maps to error",
			status:   StatusConflict,
			expected: "error",
		},
		{
			name:     "unknown maps to queue",
			status:   StatusUnknown,
			expected: "queue",
		},
		{
			name:     "invalid status maps to queue",
			status:   OperationStatus("invalid"),
			expected: "queue",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := mapOperationStatusToDisplayStatus(tt.status)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestGenerateHandlerMessage(t *testing.T) {
	// Create a time for testing
	testTime := time.Date(2023, 12, 25, 10, 0, 0, 0, time.UTC)

	tests := []struct {
		name         string
		handlerName  string
		filePath     string
		status       string
		lastExecuted *time.Time
		wantContains string
	}{
		// Symlink tests
		{
			name:         "symlink success with time",
			handlerName:  "symlink",
			filePath:     "/path/to/.vimrc",
			status:       "success",
			lastExecuted: &testTime,
			wantContains: "linked to $HOME/.vimrc",
		},
		{
			name:         "symlink success no time",
			handlerName:  "symlink",
			filePath:     "/path/to/.vimrc",
			status:       "success",
			lastExecuted: nil,
			wantContains: "linked to .vimrc",
		},
		{
			name:         "symlink error",
			handlerName:  "symlink",
			filePath:     "/path/to/.vimrc",
			status:       "error",
			wantContains: "failed to link to $HOME/.vimrc",
		},
		{
			name:         "symlink queue",
			handlerName:  "symlink",
			filePath:     "/path/to/.vimrc",
			status:       "queue",
			wantContains: "will be linked to $HOME/.vimrc",
		},
		// Shell profile tests
		{
			name:         "shell_profile success with time",
			handlerName:  "shell_profile",
			filePath:     "/path/to/.bashrc",
			status:       "success",
			lastExecuted: &testTime,
			wantContains: "included in shell profile",
		},
		{
			name:         "shell_profile success no time",
			handlerName:  "shell_profile",
			filePath:     "/path/to/.bashrc",
			status:       "success",
			lastExecuted: nil,
			wantContains: "added to shell profile",
		},
		{
			name:         "shell_profile error",
			handlerName:  "shell_profile",
			filePath:     "/path/to/.bashrc",
			status:       "error",
			wantContains: "failed to add to shell profile",
		},
		{
			name:         "shell_add_path queue",
			handlerName:  "shell_add_path",
			filePath:     "/path/to/dir",
			status:       "queue",
			wantContains: "to be included in shell profile",
		},
		// Homebrew tests
		{
			name:         "homebrew success with time",
			handlerName:  "homebrew",
			filePath:     "/path/to/Brewfile",
			status:       "success",
			lastExecuted: &testTime,
			wantContains: "executed on 2023-12-25",
		},
		{
			name:         "homebrew success no time",
			handlerName:  "homebrew",
			filePath:     "/path/to/Brewfile",
			status:       "success",
			lastExecuted: nil,
			wantContains: "packages installed",
		},
		{
			name:         "homebrew error",
			handlerName:  "homebrew",
			filePath:     "/path/to/Brewfile",
			status:       "error",
			wantContains: "failed to install packages",
		},
		{
			name:         "homebrew queue",
			handlerName:  "homebrew",
			filePath:     "/path/to/Brewfile",
			status:       "queue",
			wantContains: "packages to be installed",
		},
		// Path tests
		{
			name:         "path success",
			handlerName:  "path",
			filePath:     "/usr/local/bin",
			status:       "success",
			wantContains: "added bin to $PATH",
		},
		{
			name:         "path error",
			handlerName:  "path",
			filePath:     "/usr/local/bin",
			status:       "error",
			wantContains: "failed to add bin to $PATH",
		},
		{
			name:         "path queue",
			handlerName:  "path",
			filePath:     "/usr/local/bin",
			status:       "queue",
			wantContains: "bin to be added to $PATH",
		},
		// Install tests
		{
			name:         "install success with time",
			handlerName:  "install",
			filePath:     "/path/to/install.sh",
			status:       "success",
			lastExecuted: &testTime,
			wantContains: "executed during installation on 2023-12-25",
		},
		{
			name:         "install_script success no time",
			handlerName:  "install_script",
			filePath:     "/path/to/install.sh",
			status:       "success",
			lastExecuted: nil,
			wantContains: "installation completed",
		},
		{
			name:         "install error",
			handlerName:  "install",
			filePath:     "/path/to/install.sh",
			status:       "error",
			wantContains: "installation failed",
		},
		{
			name:         "install queue",
			handlerName:  "install",
			filePath:     "/path/to/install.sh",
			status:       "queue",
			wantContains: "to be executed during installation",
		},
		// Unknown handler tests
		{
			name:         "unknown handler success",
			handlerName:  "custom",
			filePath:     "/path/to/file",
			status:       "success",
			wantContains: "completed successfully",
		},
		{
			name:         "unknown handler error",
			handlerName:  "custom",
			filePath:     "/path/to/file",
			status:       "error",
			wantContains: "execution failed",
		},
		{
			name:         "unknown handler queue",
			handlerName:  "custom",
			filePath:     "/path/to/file",
			status:       "queue",
			wantContains: "pending execution",
		},
		{
			name:         "unknown handler unknown status",
			handlerName:  "custom",
			filePath:     "/path/to/file",
			status:       "unknown",
			wantContains: "pending execution",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := generateHandlerMessage(tt.handlerName, tt.filePath, tt.status, tt.lastExecuted)
			assert.Contains(t, result, tt.wantContains)
		})
	}
}

func TestExecutionContext_ToDisplayResult(t *testing.T) {
	ec := NewExecutionContext("link", true)

	// Create test packs
	vimPack := &Pack{
		Name: "vim",
		Path: "/home/user/.dotfiles/vim",
		Config: PackConfig{
			Override: []OverrideRule{
				{Path: ".vimrc", Handler: "symlink"},
			},
		},
	}

	zshPack := &Pack{
		Name: "zsh",
		Path: "/home/user/.dotfiles/zsh",
	}

	// Create pack results
	vimResult := NewPackExecutionResult(vimPack)
	testTime := time.Now()
	vimResult.AddHandlerResult(&HandlerResult{
		HandlerName: "symlink",
		Files:       []string{".vimrc", ".vim/colors"},
		Status:      StatusReady,
		EndTime:     testTime,
		Pack:        "vim",
	})
	vimResult.AddHandlerResult(&HandlerResult{
		HandlerName: "homebrew",
		Files:       []string{"Brewfile"},
		Status:      StatusError,
		Error:       errors.New("brew failed"),
		Pack:        "vim",
	})

	zshResult := NewPackExecutionResult(zshPack)
	zshResult.AddHandlerResult(&HandlerResult{
		HandlerName: "shell_profile",
		Files:       []string{".zshrc"},
		Status:      StatusSkipped,
		Pack:        "zsh",
	})

	// Add pack results
	ec.AddPackResult("vim", vimResult)
	ec.AddPackResult("zsh", zshResult)
	ec.Complete()

	// Convert to display result
	dr := ec.ToDisplayResult()

	// Verify basic properties
	assert.Equal(t, "link", dr.Command)
	assert.True(t, dr.DryRun)
	assert.Equal(t, ec.EndTime, dr.Timestamp)

	// Verify packs are sorted
	require.Len(t, dr.Packs, 2)
	assert.Equal(t, "vim", dr.Packs[0].Name)
	assert.Equal(t, "zsh", dr.Packs[1].Name)

	// Verify vim pack details
	vimDisplay := dr.Packs[0]
	assert.Equal(t, "alert", vimDisplay.Status) // Has error
	assert.False(t, vimDisplay.HasConfig)       // No config file
	assert.False(t, vimDisplay.IsIgnored)

	// Find the .vimrc file and verify override flag
	var vimrcFile *DisplayFile
	for i := range vimDisplay.Files {
		if vimDisplay.Files[i].Path == ".vimrc" {
			vimrcFile = &vimDisplay.Files[i]
			break
		}
	}
	require.NotNil(t, vimrcFile)
	assert.True(t, vimrcFile.IsOverride)
	assert.Equal(t, "symlink", vimrcFile.Handler)
	assert.Equal(t, "success", vimrcFile.Status)
	assert.NotNil(t, vimrcFile.LastExecuted)

	// Verify zsh pack
	zshDisplay := dr.Packs[1]
	assert.Equal(t, "queue", zshDisplay.Status) // Skipped
	assert.Len(t, zshDisplay.Files, 1)
	assert.Equal(t, ".zshrc", zshDisplay.Files[0].Path)
	assert.Equal(t, "queue", zshDisplay.Files[0].Status)
	assert.Nil(t, zshDisplay.Files[0].LastExecuted)
}

func TestExecutionContext_ToDisplayResult_WithConfigFiles(t *testing.T) {
	// This test would require mocking the filesystem for checkPackConfiguration
	// Since we're focusing on pure functions, we'll skip this test
	t.Skip("Requires filesystem mocking")
}

func TestCheckPackConfiguration(t *testing.T) {
	// This function requires filesystem access
	// We'll test edge cases that don't require FS

	tests := []struct {
		name          string
		pack          *Pack
		wantHasConfig bool
		wantIsIgnored bool
	}{
		{
			name:          "nil pack",
			pack:          nil,
			wantHasConfig: false,
			wantIsIgnored: false,
		},
		{
			name:          "pack with empty path",
			pack:          &Pack{Name: "test", Path: ""},
			wantHasConfig: false,
			wantIsIgnored: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			hasConfig, isIgnored := checkPackConfiguration(tt.pack)
			assert.Equal(t, tt.wantHasConfig, hasConfig)
			assert.Equal(t, tt.wantIsIgnored, isIgnored)
		})
	}
}

func TestExecutionContext_EmptyToDisplayResult(t *testing.T) {
	// Test with empty execution context
	ec := NewExecutionContext("status", false)
	ec.Complete()

	dr := ec.ToDisplayResult()

	assert.Equal(t, "status", dr.Command)
	assert.False(t, dr.DryRun)
	assert.Empty(t, dr.Packs)
	assert.Equal(t, ec.EndTime, dr.Timestamp)
}

func TestPackExecutionResult_EdgeCases(t *testing.T) {
	t.Run("nil pack", func(t *testing.T) {
		per := NewPackExecutionResult(nil)
		assert.Nil(t, per.Pack)
		assert.Equal(t, ExecutionStatusPending, per.Status)
	})

	t.Run("adding result with unknown status", func(t *testing.T) {
		pack := &Pack{Name: "test"}
		per := NewPackExecutionResult(pack)

		// Add result with empty/unknown status
		per.AddHandlerResult(&HandlerResult{
			HandlerName: "test",
			Status:      OperationStatus(""),
		})
		assert.Equal(t, 1, per.TotalHandlers)
		assert.Equal(t, 1, len(per.HandlerResults))
		// Unknown status doesn't count as completed/failed/skipped
		assert.Equal(t, 0, per.CompletedHandlers)
		assert.Equal(t, 0, per.FailedHandlers)
		assert.Equal(t, 0, per.SkippedHandlers)
	})
}

func TestHandlerResult_FindOverride(t *testing.T) {
	// Test FindOverride method on PackConfig
	pc := PackConfig{
		Override: []OverrideRule{
			{Path: ".vimrc", Handler: "symlink"},
			{Path: "*.sh", Handler: "install"},
		},
	}

	tests := []struct {
		name     string
		filename string
		wantRule *OverrideRule
	}{
		{
			name:     "exact match",
			filename: ".vimrc",
			wantRule: &OverrideRule{Path: ".vimrc", Handler: "symlink"},
		},
		{
			name:     "pattern match",
			filename: "install.sh",
			wantRule: &OverrideRule{Path: "*.sh", Handler: "install"},
		},
		{
			name:     "no match",
			filename: "README.md",
			wantRule: nil,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rule := pc.FindOverride(tt.filename)
			if tt.wantRule == nil {
				assert.Nil(t, rule)
			} else {
				require.NotNil(t, rule)
				assert.Equal(t, tt.wantRule.Path, rule.Path)
				assert.Equal(t, tt.wantRule.Handler, rule.Handler)
			}
		})
	}
}
