// pkg/commands/link/link_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test link command orchestration without real filesystem

package link_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/link"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestLinkPacks_SinglePack_Integration(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

	// Setup vim pack with common dotfiles pattern
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			"vimrc":              "\" Vim configuration\nset number",
			"gvimrc":             "\" GUI vim config",
			"colors/monokai.vim": "colorscheme monokai",
		},
		Rules: []testutil.Rule{
			{Type: "filename", Pattern: ".*rc", Handler: "symlink"},
			{Type: "directory", Pattern: "colors", Handler: "symlink"},
		},
	})

	t.Run("successful_linking", func(t *testing.T) {
		result, err := link.LinkPacks(link.LinkPacksOptions{
			DotfilesRoot:       env.DotfilesRoot,
			PackNames:          []string{"vim"},
			DryRun:             false,
			EnableHomeSymlinks: true,
		})

		if err != nil {
			t.Fatalf("Execute failed: %v", err)
		}

		// Verify execution context
		if result.Command != "link" {
			t.Errorf("expected command 'link', got %q", result.Command)
		}
		if result.DryRun {
			t.Error("should not be dry run")
		}

		// Verify pack results
		if len(result.PackResults) != 1 {
			t.Fatalf("expected 1 pack result, got %d", len(result.PackResults))
		}

		packResult := result.PackResults[0]
		if packResult.Pack.Name != "vim" {
			t.Errorf("expected pack 'vim', got %q", packResult.Pack.Name)
		}
		if packResult.Status != types.ExecutionStatusSuccess {
			t.Errorf("expected success status, got %v", packResult.Status)
		}

		// Verify datastore was called
		mockDS := env.DataStore.(*testutil.MockDataStore)
		calls := mockDS.GetCalls()

		linkCallFound := false
		for _, call := range calls {
			if call == "Link(vim,vimrc)" || call == "Link(vim,gvimrc)" {
				linkCallFound = true
				break
			}
		}
		if !linkCallFound {
			t.Error("expected Link() to be called for vim files")
		}
	})

	t.Run("dry_run_does_not_create_links", func(t *testing.T) {
		// Reset mock to clear previous calls
		env.DataStore.(*testutil.MockDataStore).Reset()

		result, err := link.LinkPacks(link.LinkPacksOptions{
			DotfilesRoot: env.DotfilesRoot,
			PackNames:    []string{"vim"},
			DryRun:       true,
			DataStore:    env.DataStore,
			FS:           env.FS,
			Paths:        env.Paths,
		})

		if err != nil {
			t.Fatalf("Execute failed: %v", err)
		}

		// Verify it's marked as dry run
		if !result.DryRun {
			t.Error("should be dry run")
		}

		// Verify no actual links were created
		mockDS := env.DataStore.(*testutil.MockDataStore)
		calls := mockDS.GetCalls()

		for _, call := range calls {
			if call == "Link(vim,vimrc)" {
				t.Error("Link() should not be called in dry run")
			}
		}
	})
}

func TestLinkPacks_MultiplePacks_Integration(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

	// Setup multiple packs
	env.SetupPack("vim", testutil.VimPack())
	env.SetupPack("git", testutil.GitPack())
	env.SetupPack("tools", testutil.ToolsPack())

	result, err := link.LinkPacks(link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"vim", "git"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	})

	if err != nil {
		t.Fatalf("Execute failed: %v", err)
	}

	// Should have results for both requested packs
	if len(result.PackResults) != 2 {
		t.Fatalf("expected 2 pack results, got %d", len(result.PackResults))
	}

	// Verify both packs were processed
	packNames := make(map[string]bool)
	for _, pr := range result.PackResults {
		packNames[pr.Pack.Name] = true
	}

	if !packNames["vim"] {
		t.Error("vim pack not processed")
	}
	if !packNames["git"] {
		t.Error("git pack not processed")
	}
	if packNames["tools"] {
		t.Error("tools pack should not be processed")
	}
}

func TestLinkPacks_ConflictDetection(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

	// Create two packs with conflicting files
	env.SetupPack("app1", testutil.PackConfig{
		Files: map[string]string{
			"config": "app1 config",
		},
		Rules: []testutil.Rule{
			{Type: "filename", Pattern: "config", Handler: "symlink"},
		},
	})

	env.SetupPack("app2", testutil.PackConfig{
		Files: map[string]string{
			"config": "app2 config",
		},
		Rules: []testutil.Rule{
			{Type: "filename", Pattern: "config", Handler: "symlink"},
		},
	})

	// Pre-configure the mock to simulate that app1's config is already linked
	mockDS := env.DataStore.(*testutil.MockDataStore)
	app1ConfigPath := filepath.Join(env.DotfilesRoot, "app1", "config")
	mockDS.WithLink("app1", "config", filepath.Join(env.HomeDir, ".config"))

	// Also create the symlink in the filesystem for consistency
	targetPath := filepath.Join(env.HomeDir, ".config")
	env.FS.Symlink(app1ConfigPath, targetPath)

	// Now try to link app2 - should detect conflict
	result, err := link.LinkPacks(link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"app2"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	})

	// The command should succeed but report a conflict
	if err != nil {
		t.Fatalf("Command failed unexpectedly: %v", err)
	}

	if len(result.PackResults) == 0 {
		t.Fatal("Expected pack results")
	}

	packResult := result.PackResults[0]
	
	// Check if conflict was detected
	hasConflict := false
	for _, hr := range packResult.HandlerResults {
		if hr.Status == types.StatusConflicted {
			hasConflict = true
			break
		}
	}
	
	if !hasConflict {
		t.Error("Expected conflict to be detected when linking app2")
	}
}

func TestLinkPacks_ErrorHandling(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

	t.Run("pack_not_found", func(t *testing.T) {
		_, err := link.LinkPacks(link.LinkPacksOptions{
			DotfilesRoot:       env.DotfilesRoot,
			PackNames:          []string{"nonexistent"},
			DryRun:             false,
			EnableHomeSymlinks: true,
		})

		if err == nil {
			t.Error("should error on non-existent pack")
		}
	})

	t.Run("datastore_error", func(t *testing.T) {
		env.SetupPack("vim", testutil.VimPack())

		// Configure mock to return error
		mockDS := env.DataStore.(*testutil.MockDataStore)
		mockDS.WithError("Link", errors.New(errors.ErrInternal, "datastore failure"))

		result, err := link.LinkPacks(link.LinkPacksOptions{
			DotfilesRoot:       env.DotfilesRoot,
			PackNames:          []string{"vim"},
			DryRun:             false,
			EnableHomeSymlinks: true,
		})

		// Command may complete but actions should fail
		if err == nil && len(result.PackResults) > 0 {
			packResult := result.PackResults[0]
			if packResult.Status == types.ExecutionStatusSuccess {
				t.Error("pack should not succeed with datastore errors")
			}
		}
	})
}
