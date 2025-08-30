package internal

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil_old"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestRunPipeline_Deploy(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "pipeline-deploy")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create a test pack with files
	testutil.CreateDir(t, dotfilesDir, "vim")
	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "\" Test vimrc")
	testutil.CreateFile(t, dotfilesDir, "vim/gvimrc", "\" Test gvimrc")

	// Run pipeline in deploy mode (configuration handlers)
	opts := PipelineOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"vim"},
		DryRun:             false,
		CommandMode:        CommandModeConfiguration,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	ctx, err := RunPipeline(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify context
	testutil.AssertEqual(t, "link", ctx.Command)
	testutil.AssertFalse(t, ctx.DryRun, "Should not be dry run")

	// Verify pack results
	packResult, ok := ctx.GetPackResult("vim")
	testutil.AssertTrue(t, ok, "Should have vim pack result")
	testutil.AssertNotNil(t, packResult)
	testutil.AssertEqual(t, "vim", packResult.Pack.Name)

	// Should have symlink handler
	testutil.AssertTrue(t, len(packResult.HandlerResults) > 0, "Should have handler results")

	// Verify files were created (Layer 1: top-level files get dot prefix)
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".vimrc")), "vimrc symlink should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".gvimrc")), "gvimrc symlink should exist")
}

func TestRunPipeline_DryRun(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "pipeline-dryrun")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)

	// Create a test pack
	testutil.CreateDir(t, dotfilesDir, "bash")
	testutil.CreateFile(t, dotfilesDir, "bash/bashrc", "# Test bashrc")

	// Run pipeline in dry run mode
	opts := PipelineOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{}, // All packs
		DryRun:             true,
		CommandMode:        CommandModeConfiguration,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	ctx, err := RunPipeline(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify context
	testutil.AssertTrue(t, ctx.DryRun, "Should be dry run")

	// Verify pack results exist but no files created
	packResult, ok := ctx.GetPackResult("bash")
	testutil.AssertTrue(t, ok, "Should have bash pack result")
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, packResult.Status)

	// Verify no files were created
	testutil.AssertFalse(t, testutil.FileExists(t, filepath.Join(homeDir, "bashrc")), "bashrc symlink should not exist in dry run")
}

func TestRunPipeline_Install(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "pipeline-install")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create pack with install script
	testutil.CreateDir(t, dotfilesDir, "tools")
	installScript := `#!/bin/bash
echo "Installing tools"
`
	testutil.CreateFile(t, dotfilesDir, "tools/install.sh", installScript)
	err := os.Chmod(filepath.Join(dotfilesDir, "tools/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Run pipeline in install mode (code execution handlers)
	opts := PipelineOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"tools"},
		DryRun:             false,
		CommandMode:        CommandModeAll,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	ctx, err := RunPipeline(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify context
	testutil.AssertEqual(t, "provision", ctx.Command)

	// Verify pack results
	packResult, ok := ctx.GetPackResult("tools")
	testutil.AssertTrue(t, ok, "Should have tools pack result")

	// Handlers use generic "handler" name
	// Should have at least one handler result for the install script
	testutil.AssertTrue(t, len(packResult.HandlerResults) > 0, "Should have handler results")
}

func TestRunPipeline_InvalidPack(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "pipeline-invalid")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")

	testutil.CreateDir(t, tempDir, "dotfiles")

	t.Setenv("HOME", tempDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)

	// Try to run pipeline with non-existent pack
	opts := PipelineOptions{
		DotfilesRoot: dotfilesDir,
		PackNames:    []string{"nonexistent"},
		DryRun:       false,
		CommandMode:  CommandModeConfiguration,
		Force:        false,
	}

	ctx, err := RunPipeline(opts)
	testutil.AssertError(t, err)
	// The error will be about pack not found
	testutil.AssertTrue(t,
		strings.Contains(err.Error(), "nonexistent") || strings.Contains(err.Error(), "pack(s) not found"),
		"Error should mention nonexistent pack or pack not found")

	// Context should still be returned even on error
	if ctx != nil {
		testutil.AssertTrue(t, ctx.EndTime.After(ctx.StartTime), "End time should be set")
	}
}

// TestFilterActionsByHandlerType is now tested in pkg/core/actions_test.go
// since the functionality moved there with the actions
