package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestExecutionContext_ExecuteChecksumOperations(t *testing.T) {
	// Create a temporary test file
	tempDir := testutil.TempDir(t, "checksum-test")
	testFile := filepath.Join(tempDir, "test.txt")
	testContent := "Hello, World!"
	
	err := os.WriteFile(testFile, []byte(testContent), 0644)
	testutil.AssertNoError(t, err)

	// Create execution context
	ctx := NewExecutionContext()

	// Create checksum operation
	ops := []types.Operation{
		{
			Type:        types.OperationChecksum,
			Source:      testFile,
			Description: "Test checksum",
		},
	}

	// Execute checksum operations
	results, err := ctx.ExecuteChecksumOperations(ops)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(results))

	// Verify result
	result := results[0]
	testutil.AssertTrue(t, result.Success)
	testutil.AssertNoError(t, result.Error)
	
	checksum, ok := result.Result.(string)
	testutil.AssertTrue(t, ok)
	testutil.AssertNotEmpty(t, checksum)
	
	// Verify checksum is stored in context
	storedChecksum, exists := ctx.GetChecksum(testFile)
	testutil.AssertTrue(t, exists)
	testutil.AssertEqual(t, checksum, storedChecksum)
	
	// Verify it's a valid SHA256 checksum (64 hex characters)
	testutil.AssertEqual(t, 64, len(checksum))
}

func TestExecutionContext_GetChecksum(t *testing.T) {
	ctx := NewExecutionContext()
	
	// Test with no checksum stored
	_, exists := ctx.GetChecksum("/nonexistent/file")
	testutil.AssertFalse(t, exists)
	
	// Store a checksum
	testPath := "/test/path/file.txt"
	testChecksum := "abcd1234567890abcd1234567890abcd1234567890abcd1234567890abcd1234"
	ctx.ChecksumResults[testPath] = testChecksum
	
	// Retrieve the checksum
	checksum, exists := ctx.GetChecksum(testPath)
	testutil.AssertTrue(t, exists)
	testutil.AssertEqual(t, testChecksum, checksum)
}

func TestExecutionContext_ExecuteChecksumOperations_FileNotFound(t *testing.T) {
	ctx := NewExecutionContext()

	// Create checksum operation for non-existent file
	ops := []types.Operation{
		{
			Type:        types.OperationChecksum,
			Source:      "/nonexistent/file.txt",
			Description: "Test checksum for missing file",
		},
	}

	// Execute checksum operations
	results, err := ctx.ExecuteChecksumOperations(ops)
	testutil.AssertError(t, err)
	testutil.AssertEqual(t, 1, len(results))

	// Verify result shows failure
	result := results[0]
	testutil.AssertFalse(t, result.Success)
	testutil.AssertError(t, result.Error)
}

func TestCalculateFileChecksum(t *testing.T) {
	// Create a temporary test file with known content
	tempDir := testutil.TempDir(t, "checksum-calc-test")
	testFile := filepath.Join(tempDir, "test.txt")
	testContent := "Hello, World!"
	
	err := os.WriteFile(testFile, []byte(testContent), 0644)
	testutil.AssertNoError(t, err)

	// Calculate checksum
	checksum, err := calculateFileChecksum(testFile)
	testutil.AssertNoError(t, err)
	
	// Verify it's a valid SHA256 checksum (64 hex characters)
	testutil.AssertEqual(t, 64, len(checksum))
	
	// Verify it's the expected checksum for "Hello, World!"
	// echo -n "Hello, World!" | sha256sum
	expectedChecksum := "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
	testutil.AssertEqual(t, expectedChecksum, checksum)
}

func TestConvertBrewActionWithContext(t *testing.T) {
	// Create a temporary test file
	tempDir := testutil.TempDir(t, "brew-action-test")
	brewfile := filepath.Join(tempDir, "Brewfile")
	err := os.WriteFile(brewfile, []byte("brew 'git'\n"), 0644)
	testutil.AssertNoError(t, err)

	// Create execution context and store a checksum
	ctx := NewExecutionContext()
	testChecksum := "test-checksum-12345"
	ctx.ChecksumResults[brewfile] = testChecksum

	// Create brew action
	action := types.Action{
		Type:        types.ActionTypeBrew,
		Source:      brewfile,
		Description: "Test brew action",
		Metadata: map[string]interface{}{
			"pack": "test-pack",
		},
	}

	// Convert action with context
	ops, err := convertBrewActionWithContext(action, ctx)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 2, len(ops))

	// Verify the sentinel file operation uses the real checksum
	sentinelOp := ops[1] // Second operation should be the sentinel file write
	testutil.AssertEqual(t, types.OperationWriteFile, sentinelOp.Type)
	testutil.AssertEqual(t, testChecksum, sentinelOp.Content)
}

func TestConvertInstallActionWithContext(t *testing.T) {
	// Create a temporary test file
	tempDir := testutil.TempDir(t, "install-action-test")
	installScript := filepath.Join(tempDir, "install.sh")
	err := os.WriteFile(installScript, []byte("#!/bin/bash\necho 'Installing'\n"), 0644)
	testutil.AssertNoError(t, err)

	// Create execution context and store a checksum
	ctx := NewExecutionContext()
	testChecksum := "install-checksum-67890"
	ctx.ChecksumResults[installScript] = testChecksum

	// Create install action
	action := types.Action{
		Type:        types.ActionTypeInstall,
		Source:      installScript,
		Description: "Test install action",
		Metadata: map[string]interface{}{
			"pack": "test-pack",
		},
	}

	// Convert action with context
	ops, err := convertInstallActionWithContext(action, ctx)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 2, len(ops))

	// Verify the sentinel file operation uses the real checksum
	sentinelOp := ops[1] // Second operation should be the sentinel file write
	testutil.AssertEqual(t, types.OperationWriteFile, sentinelOp.Type)
	testutil.AssertEqual(t, testChecksum, sentinelOp.Content)
}

func TestConvertActionWithoutChecksum_ReturnsError(t *testing.T) {
	// Test that brew and install actions fail without checksum
	tempDir := testutil.TempDir(t, "no-checksum-test")
	testFile := filepath.Join(tempDir, "test.txt")
	err := os.WriteFile(testFile, []byte("test"), 0644)
	testutil.AssertNoError(t, err)

	tests := []struct {
		name       string
		actionType types.ActionType
	}{
		{"brew action", types.ActionTypeBrew},
		{"install action", types.ActionTypeInstall},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create empty execution context (no checksums)
			ctx := NewExecutionContext()

			// Create action without checksum in metadata
			action := types.Action{
				Type:        tt.actionType,
				Source:      testFile,
				Description: "Test action without checksum",
				Metadata: map[string]interface{}{
					"pack": "test-pack",
				},
			}

			// Convert action with context - should fail
			_, err := ConvertActionWithContext(action, ctx)
			testutil.AssertError(t, err)
			testutil.AssertContains(t, err.Error(), "requires checksum")
		})
	}
}