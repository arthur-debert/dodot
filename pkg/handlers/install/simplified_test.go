package install_test

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/install"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSimplifiedHandler_ToOperations(t *testing.T) {
	// Create temp directory for test scripts
	tempDir := t.TempDir()

	// Create test install script
	scriptContent := "#!/bin/bash\necho 'Installing test pack'\n"
	scriptPath := filepath.Join(tempDir, "install.sh")
	err := os.WriteFile(scriptPath, []byte(scriptContent), 0755)
	require.NoError(t, err)

	handler := install.NewSimplifiedHandler()

	tests := []struct {
		name     string
		matches  []types.RuleMatch
		wantOps  int
		wantErr  bool
		checkOps func(*testing.T, []operations.Operation)
	}{
		{
			name: "single install script creates one RunCommand operation",
			matches: []types.RuleMatch{
				{
					Pack:         "testpack",
					Path:         "install.sh",
					AbsolutePath: scriptPath,
					HandlerName:  "install",
				},
			},
			wantOps: 1,
			checkOps: func(t *testing.T, ops []operations.Operation) {
				assert.Equal(t, operations.RunCommand, ops[0].Type)
				assert.Equal(t, "testpack", ops[0].Pack)
				assert.Equal(t, "install", ops[0].Handler)
				assert.Contains(t, ops[0].Command, scriptPath)
				assert.Contains(t, ops[0].Command, "bash")
				assert.NotEmpty(t, ops[0].Sentinel)
				assert.Contains(t, ops[0].Sentinel, "install.sh-")
			},
		},
		{
			name: "multiple scripts create multiple operations",
			matches: []types.RuleMatch{
				{
					Pack:         "pack1",
					Path:         "install.sh",
					AbsolutePath: scriptPath,
					HandlerName:  "install",
				},
				{
					Pack:         "pack2",
					Path:         "setup.sh",
					AbsolutePath: scriptPath,
					HandlerName:  "install",
				},
			},
			wantOps: 2,
			checkOps: func(t *testing.T, ops []operations.Operation) {
				// Both should be RunCommand operations
				for _, op := range ops {
					assert.Equal(t, operations.RunCommand, op.Type)
					assert.Equal(t, "install", op.Handler)
					assert.Contains(t, op.Command, "bash")
				}

				// Check specific packs
				assert.Equal(t, "pack1", ops[0].Pack)
				assert.Contains(t, ops[0].Sentinel, "install.sh-")

				assert.Equal(t, "pack2", ops[1].Pack)
				assert.Contains(t, ops[1].Sentinel, "setup.sh-")
			},
		},
		{
			name:    "empty matches returns empty operations",
			matches: []types.RuleMatch{},
			wantOps: 0,
		},
		{
			name: "non-existent script path returns error",
			matches: []types.RuleMatch{
				{
					Pack:         "badpack",
					Path:         "missing.sh",
					AbsolutePath: "/non/existent/script.sh",
					HandlerName:  "install",
				},
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ops, err := handler.ToOperations(tt.matches)

			if tt.wantErr {
				assert.Error(t, err)
				return
			}

			require.NoError(t, err)
			assert.Len(t, ops, tt.wantOps)

			if tt.checkOps != nil {
				tt.checkOps(t, ops)
			}
		})
	}
}

func TestSimplifiedHandler_GetMetadata(t *testing.T) {
	handler := install.NewSimplifiedHandler()
	meta := handler.GetMetadata()

	assert.Equal(t, "Runs install.sh scripts for initial setup", meta.Description)
	assert.False(t, meta.RequiresConfirm)
	assert.False(t, meta.CanRunMultiple) // Scripts run once per checksum
}

func TestSimplifiedHandler_DeterministicSentinel(t *testing.T) {
	// Create a test script with known content
	tempDir := t.TempDir()
	scriptContent := "#!/bin/bash\necho 'test'\n"
	scriptPath := filepath.Join(tempDir, "install.sh")
	err := os.WriteFile(scriptPath, []byte(scriptContent), 0755)
	require.NoError(t, err)

	handler := install.NewSimplifiedHandler()

	match := types.RuleMatch{
		Pack:         "test",
		Path:         "install.sh",
		AbsolutePath: scriptPath,
		HandlerName:  "install",
	}

	// Generate operations multiple times
	ops1, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	ops2, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	// Sentinels should be identical for same content
	assert.Equal(t, ops1[0].Sentinel, ops2[0].Sentinel)

	// Modify the script
	newContent := "#!/bin/bash\necho 'modified'\n"
	err = os.WriteFile(scriptPath, []byte(newContent), 0755)
	require.NoError(t, err)

	ops3, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	// Sentinel should be different after modification
	assert.NotEqual(t, ops1[0].Sentinel, ops3[0].Sentinel)
}

func TestSimplifiedHandler_CommandFormat(t *testing.T) {
	tempDir := t.TempDir()

	// Test with path containing spaces
	scriptPath := filepath.Join(tempDir, "my install script.sh")
	err := os.WriteFile(scriptPath, []byte("#!/bin/bash\necho 'test'\n"), 0755)
	require.NoError(t, err)

	handler := install.NewSimplifiedHandler()

	match := types.RuleMatch{
		Pack:         "test",
		Path:         "my install script.sh",
		AbsolutePath: scriptPath,
		HandlerName:  "install",
	}

	ops, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	// Command should properly quote the path
	expectedCommand := fmt.Sprintf("bash '%s'", scriptPath)
	assert.Equal(t, expectedCommand, ops[0].Command)
}

func TestSimplifiedHandler_GetTemplateContent(t *testing.T) {
	handler := install.NewSimplifiedHandler()

	// Template should not be empty
	template := handler.GetTemplateContent()
	assert.NotEmpty(t, template)

	// Should contain install script markers
	assert.Contains(t, template, "#!/usr/bin/env bash")
	assert.Contains(t, template, "dodot install script")
	assert.Contains(t, template, "PACK_NAME")
}

func TestSimplifiedHandler_FormatClearedItem(t *testing.T) {
	handler := install.NewSimplifiedHandler()

	item := types.ClearedItem{
		Type:        "provision_state",
		Path:        "/some/path",
		Description: "Default description",
	}

	// Test dry run formatting
	formatted := handler.FormatClearedItem(item, true)
	assert.Equal(t, "Would remove install run records", formatted)

	// Test actual run formatting
	formatted = handler.FormatClearedItem(item, false)
	assert.Equal(t, "Removing install run records", formatted)
}

func TestSimplifiedHandler_SentinelFormat(t *testing.T) {
	// This test verifies the sentinel format is consistent
	tempDir := t.TempDir()
	scriptPath := filepath.Join(tempDir, "test.sh")
	err := os.WriteFile(scriptPath, []byte("#!/bin/bash\necho 'test'\n"), 0755)
	require.NoError(t, err)

	handler := install.NewSimplifiedHandler()

	match := types.RuleMatch{
		Pack:         "test",
		Path:         "subdir/test.sh",
		AbsolutePath: scriptPath,
		HandlerName:  "install",
	}

	ops, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	// Sentinel should use basename of path, not full path
	assert.True(t, strings.HasPrefix(ops[0].Sentinel, "test.sh-"))
	assert.False(t, strings.Contains(ops[0].Sentinel, "subdir"))
}
