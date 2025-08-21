package core

import (
	"fmt"
	"path/filepath"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestPackConfigurationDetection(t *testing.T) {
	tests := []struct {
		name         string
		setupPack    func(string) *types.Pack
		expectConfig bool
		expectIgnore bool
	}{
		{
			name: "no_config_files",
			setupPack: func(tempDir string) *types.Pack {
				packDir := filepath.Join(tempDir, "test-pack")
				testutil.CreateDir(t, tempDir, "test-pack")
				return &types.Pack{
					Name: "test-pack",
					Path: packDir,
				}
			},
			expectConfig: false,
			expectIgnore: false,
		},
		{
			name: "has_config_only",
			setupPack: func(tempDir string) *types.Pack {
				packDir := filepath.Join(tempDir, "test-pack")
				testutil.CreateDir(t, tempDir, "test-pack")
				testutil.CreateFile(t, packDir, ".dodot.toml", "# Test config")
				return &types.Pack{
					Name: "test-pack",
					Path: packDir,
				}
			},
			expectConfig: true,
			expectIgnore: false,
		},
		{
			name: "has_ignore_only",
			setupPack: func(tempDir string) *types.Pack {
				packDir := filepath.Join(tempDir, "test-pack")
				testutil.CreateDir(t, tempDir, "test-pack")
				testutil.CreateFile(t, packDir, ".dodotignore", "*.tmp")
				return &types.Pack{
					Name: "test-pack",
					Path: packDir,
				}
			},
			expectConfig: false,
			expectIgnore: true,
		},
		{
			name: "has_both_config_and_ignore",
			setupPack: func(tempDir string) *types.Pack {
				packDir := filepath.Join(tempDir, "test-pack")
				testutil.CreateDir(t, tempDir, "test-pack")
				testutil.CreateFile(t, packDir, ".dodot.toml", "# Test config")
				testutil.CreateFile(t, packDir, ".dodotignore", "*.tmp")
				return &types.Pack{
					Name: "test-pack",
					Path: packDir,
				}
			},
			expectConfig: true,
			expectIgnore: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tempDir := testutil.TempDir(t, "pack-config-test")
			pack := tt.setupPack(tempDir)

			// Create ExecutionContext and test ToDisplayResult
			ctx := types.NewExecutionContext("test", false)
			packResult := types.NewPackExecutionResult(pack)
			ctx.AddPackResult(pack.Name, packResult)
			ctx.Complete()

			displayResult := ctx.ToDisplayResult()

			testutil.AssertEqual(t, 1, len(displayResult.Packs))
			displayPack := displayResult.Packs[0]

			testutil.AssertEqual(t, tt.expectConfig, displayPack.HasConfig)
			testutil.AssertEqual(t, tt.expectIgnore, displayPack.IsIgnored)
		})
	}
}

func TestToDisplayResult_PackConfiguration(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "display-result-config-test")

	// Create pack with config file
	packWithConfigDir := filepath.Join(tempDir, "pack-with-config")
	testutil.CreateDir(t, tempDir, "pack-with-config")
	testutil.CreateFile(t, packWithConfigDir, ".dodot.toml", "# Test config")

	// Create pack with ignore file
	packWithIgnoreDir := filepath.Join(tempDir, "pack-with-ignore")
	testutil.CreateDir(t, tempDir, "pack-with-ignore")
	testutil.CreateFile(t, packWithIgnoreDir, ".dodotignore", "*.tmp")

	// Create pack with no config files
	packNoConfigDir := filepath.Join(tempDir, "pack-no-config")
	testutil.CreateDir(t, tempDir, "pack-no-config")

	// Create ExecutionContext
	ctx := types.NewExecutionContext("deploy", false)

	// Add pack results
	packWithConfig := &types.Pack{Name: "pack-with-config", Path: packWithConfigDir}
	packWithIgnore := &types.Pack{Name: "pack-with-ignore", Path: packWithIgnoreDir}
	packNoConfig := &types.Pack{Name: "pack-no-config", Path: packNoConfigDir}

	ctx.AddPackResult("pack-with-config", types.NewPackExecutionResult(packWithConfig))
	ctx.AddPackResult("pack-with-ignore", types.NewPackExecutionResult(packWithIgnore))
	ctx.AddPackResult("pack-no-config", types.NewPackExecutionResult(packNoConfig))
	ctx.Complete()

	// Transform to DisplayResult
	displayResult := ctx.ToDisplayResult()

	// Verify results
	testutil.AssertEqual(t, 3, len(displayResult.Packs))

	// Find packs in result (order might vary)
	var configPack, ignorePack, noConfigPack *types.DisplayPack
	for i := range displayResult.Packs {
		switch displayResult.Packs[i].Name {
		case "pack-with-config":
			configPack = &displayResult.Packs[i]
		case "pack-with-ignore":
			ignorePack = &displayResult.Packs[i]
		case "pack-no-config":
			noConfigPack = &displayResult.Packs[i]
		}
	}

	// Verify config detection
	testutil.AssertTrue(t, configPack != nil, "pack-with-config should exist")
	testutil.AssertTrue(t, configPack.HasConfig, "pack-with-config should have HasConfig=true")
	testutil.AssertFalse(t, configPack.IsIgnored, "pack-with-config should have IsIgnored=false")

	// Verify ignore detection
	testutil.AssertTrue(t, ignorePack != nil, "pack-with-ignore should exist")
	testutil.AssertFalse(t, ignorePack.HasConfig, "pack-with-ignore should have HasConfig=false")
	testutil.AssertTrue(t, ignorePack.IsIgnored, "pack-with-ignore should have IsIgnored=true")

	// Verify no config pack
	testutil.AssertTrue(t, noConfigPack != nil, "pack-no-config should exist")
	testutil.AssertFalse(t, noConfigPack.HasConfig, "pack-no-config should have HasConfig=false")
	testutil.AssertFalse(t, noConfigPack.IsIgnored, "pack-no-config should have IsIgnored=false")
}

func TestToDisplayResult_ConfigFilesAsDisplayItems(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "display-items-test")

	// Create pack with config file
	packWithConfigDir := filepath.Join(tempDir, "pack-with-config")
	testutil.CreateDir(t, tempDir, "pack-with-config")
	testutil.CreateFile(t, packWithConfigDir, ".dodot.toml", "# Test config")

	// Create pack with ignore file
	packWithIgnoreDir := filepath.Join(tempDir, "pack-with-ignore")
	testutil.CreateDir(t, tempDir, "pack-with-ignore")
	testutil.CreateFile(t, packWithIgnoreDir, ".dodotignore", "*.tmp")

	// Create pack with both files
	packWithBothDir := filepath.Join(tempDir, "pack-with-both")
	testutil.CreateDir(t, tempDir, "pack-with-both")
	testutil.CreateFile(t, packWithBothDir, ".dodot.toml", "# Test config")
	testutil.CreateFile(t, packWithBothDir, ".dodotignore", "*.tmp")

	// Create ExecutionContext
	ctx := types.NewExecutionContext("deploy", false)

	// Add pack results
	packWithConfig := &types.Pack{Name: "pack-with-config", Path: packWithConfigDir}
	packWithIgnore := &types.Pack{Name: "pack-with-ignore", Path: packWithIgnoreDir}
	packWithBoth := &types.Pack{Name: "pack-with-both", Path: packWithBothDir}

	ctx.AddPackResult("pack-with-config", types.NewPackExecutionResult(packWithConfig))
	ctx.AddPackResult("pack-with-ignore", types.NewPackExecutionResult(packWithIgnore))
	ctx.AddPackResult("pack-with-both", types.NewPackExecutionResult(packWithBoth))
	ctx.Complete()

	// Transform to DisplayResult
	displayResult := ctx.ToDisplayResult()

	// Verify results
	testutil.AssertEqual(t, 3, len(displayResult.Packs))

	// Find packs in result
	packMap := make(map[string]*types.DisplayPack)
	for i := range displayResult.Packs {
		packMap[displayResult.Packs[i].Name] = &displayResult.Packs[i]
	}

	// Test pack with config file
	configPack := packMap["pack-with-config"]
	testutil.AssertTrue(t, configPack != nil, "pack-with-config should exist")
	testutil.AssertEqual(t, 1, len(configPack.Files))

	configFile := configPack.Files[0]
	testutil.AssertEqual(t, "config", configFile.Handler)
	testutil.AssertEqual(t, ".dodot.toml", configFile.Path)
	testutil.AssertEqual(t, "config", configFile.Status)
	testutil.AssertEqual(t, "dodot config file found", configFile.Message)

	// Test pack with ignore file
	ignorePack := packMap["pack-with-ignore"]
	testutil.AssertTrue(t, ignorePack != nil, "pack-with-ignore should exist")
	testutil.AssertEqual(t, 1, len(ignorePack.Files))

	ignoreFile := ignorePack.Files[0]
	testutil.AssertEqual(t, ".dodotignore", ignoreFile.Handler)
	testutil.AssertEqual(t, "", ignoreFile.Path)
	testutil.AssertEqual(t, "ignored", ignoreFile.Status)
	testutil.AssertEqual(t, "dodot is ignoring this dir", ignoreFile.Message)

	// Test pack with both files
	bothPack := packMap["pack-with-both"]
	testutil.AssertTrue(t, bothPack != nil, "pack-with-both should exist")
	testutil.AssertEqual(t, 2, len(bothPack.Files))

	// Files should be in order: config first, then ignore
	testutil.AssertEqual(t, "config", bothPack.Files[0].Handler)
	testutil.AssertEqual(t, ".dodotignore", bothPack.Files[1].Handler)
}

func TestToDisplayResult_FileOverrideDetection(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "override-test")

	// Create pack with override configuration
	packDir := filepath.Join(tempDir, "test-pack")
	testutil.CreateDir(t, tempDir, "test-pack")

	// Create files that will have overrides
	testutil.CreateFile(t, packDir, "vimrc", "# vim config")
	testutil.CreateFile(t, packDir, "bashrc", "# bash config")
	testutil.CreateFile(t, packDir, "regular-file", "# no override")

	// Create ExecutionContext
	ctx := types.NewExecutionContext("test", false)

	// Create pack with override config
	pack := &types.Pack{
		Name: "test-pack",
		Path: packDir,
		Config: types.PackConfig{
			Override: []types.OverrideRule{
				{
					Path:    "vimrc",
					Handler: "symlink",
				},
				{
					Path:    "bash*", // Pattern match
					Handler: "shell_profile",
				},
			},
		},
	}

	packResult := types.NewPackExecutionResult(pack)

	// Add HandlerResults with different files
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{filepath.Join(packDir, "vimrc")},
		Status:      types.StatusReady,
		Message:     "linked",
	})

	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "shell_profile",
		Files:       []string{filepath.Join(packDir, "bashrc")},
		Status:      types.StatusReady,
		Message:     "included",
	})

	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "default",
		Files:       []string{filepath.Join(packDir, "regular-file")},
		Status:      types.StatusReady,
		Message:     "processed",
	})

	ctx.AddPackResult("test-pack", packResult)
	ctx.Complete()

	// Transform to DisplayResult
	displayResult := ctx.ToDisplayResult()

	// Verify results
	testutil.AssertEqual(t, 1, len(displayResult.Packs))
	pack1 := displayResult.Packs[0]

	// Should have files from HandlerResults (no config files since no .dodot.toml exists)
	testutil.AssertEqual(t, 3, len(pack1.Files))

	// Find files by path
	fileMap := make(map[string]*types.DisplayFile)
	for i := range pack1.Files {
		fileName := filepath.Base(pack1.Files[i].Path)
		fileMap[fileName] = &pack1.Files[i]
	}

	// Test vimrc has override (exact match)
	vimrcFile := fileMap["vimrc"]
	testutil.AssertTrue(t, vimrcFile != nil, "vimrc file should exist")
	testutil.AssertTrue(t, vimrcFile.IsOverride, "vimrc should have IsOverride=true")

	// Test bashrc has override (pattern match)
	bashrcFile := fileMap["bashrc"]
	testutil.AssertTrue(t, bashrcFile != nil, "bashrc file should exist")
	testutil.AssertTrue(t, bashrcFile.IsOverride, "bashrc should have IsOverride=true")

	// Test regular-file has no override
	regularFile := fileMap["regular-file"]
	testutil.AssertTrue(t, regularFile != nil, "regular-file should exist")
	testutil.AssertFalse(t, regularFile.IsOverride, "regular-file should have IsOverride=false")
}

func TestToDisplayResult_LastExecutedTimestamps(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "timestamp-test")
	packDir := filepath.Join(tempDir, "test-pack")
	testutil.CreateDir(t, tempDir, "test-pack")
	testutil.CreateFile(t, packDir, "test-file", "# test")

	// Create ExecutionContext
	ctx := types.NewExecutionContext("test", false)

	// Create pack
	pack := &types.Pack{
		Name: "test-pack",
		Path: packDir,
	}

	packResult := types.NewPackExecutionResult(pack)

	// Create HandlerResults with different statuses and timestamps
	successTime := time.Now().Add(-1 * time.Hour)

	// Successful HandlerResult with timestamp
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{filepath.Join(packDir, "test-file")},
		Status:      types.StatusReady,
		EndTime:     successTime,
		Message:     "linked successfully",
	})

	// Failed HandlerResult (should not have LastExecuted)
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "install",
		Files:       []string{filepath.Join(packDir, "install-file")},
		Status:      types.StatusError,
		EndTime:     time.Now(),
		Message:     "failed to install",
	})

	// Skipped HandlerResult (should not have LastExecuted)
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "homebrew",
		Files:       []string{filepath.Join(packDir, "Brewfile")},
		Status:      types.StatusSkipped,
		EndTime:     time.Now(),
		Message:     "skipped",
	})

	ctx.AddPackResult("test-pack", packResult)
	ctx.Complete()

	// Transform to DisplayResult
	displayResult := ctx.ToDisplayResult()

	// Verify results
	testutil.AssertEqual(t, 1, len(displayResult.Packs))
	pack1 := displayResult.Packs[0]
	testutil.AssertEqual(t, 3, len(pack1.Files))

	// Find files by Handler
	fileMap := make(map[string]*types.DisplayFile)
	for i := range pack1.Files {
		fileMap[pack1.Files[i].Handler] = &pack1.Files[i]
	}

	// Test successful HandlerResult has LastExecuted timestamp
	symlinkFile := fileMap["symlink"]
	testutil.AssertTrue(t, symlinkFile != nil, "symlink file should exist")
	testutil.AssertTrue(t, symlinkFile.LastExecuted != nil, "symlink should have LastExecuted timestamp")
	testutil.AssertTrue(t, symlinkFile.LastExecuted.Equal(successTime), "timestamp should match HandlerResult EndTime")

	// Test failed HandlerResult has no LastExecuted timestamp
	installFile := fileMap["install"]
	testutil.AssertTrue(t, installFile != nil, "install file should exist")
	testutil.AssertTrue(t, installFile.LastExecuted == nil, "failed HandlerResult should not have LastExecuted")

	// Test skipped HandlerResult has no LastExecuted timestamp
	brewFile := fileMap["homebrew"]
	testutil.AssertTrue(t, brewFile != nil, "homebrew file should exist")
	testutil.AssertTrue(t, brewFile.LastExecuted == nil, "skipped HandlerResult should not have LastExecuted")
}

func TestToDisplayResult_HandlerAwareMessages(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "handler-messages-test")
	packDir := filepath.Join(tempDir, "test-pack")
	testutil.CreateDir(t, tempDir, "test-pack")

	// Create ExecutionContext
	ctx := types.NewExecutionContext("test", false)

	pack := &types.Pack{
		Name: "test-pack",
		Path: packDir,
	}

	packResult := types.NewPackExecutionResult(pack)

	// Test different Handler types with different statuses
	testTime := time.Now().Add(-2 * time.Hour)

	// Test symlink Handler - success
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{filepath.Join(packDir, ".vimrc")},
		Status:      types.StatusReady,
		EndTime:     testTime,
	})

	// Test symlink Handler - pending
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{filepath.Join(packDir, ".bashrc")},
		Status:      types.StatusSkipped, // Maps to "queue"
	})

	// Test homebrew Handler - success with timestamp
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "homebrew",
		Files:       []string{filepath.Join(packDir, "Brewfile")},
		Status:      types.StatusReady,
		EndTime:     testTime,
	})

	// Test shell_profile Handler - success
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "shell_profile",
		Files:       []string{filepath.Join(packDir, "aliases.sh")},
		Status:      types.StatusReady,
		EndTime:     testTime,
	})

	// Test install Handler - error
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "install",
		Files:       []string{filepath.Join(packDir, "setup.sh")},
		Status:      types.StatusError,
	})

	// Test path Handler - pending
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "path",
		Files:       []string{filepath.Join(packDir, "bin")},
		Status:      types.StatusSkipped, // Maps to "queue"
	})

	ctx.AddPackResult("test-pack", packResult)
	ctx.Complete()

	// Transform to DisplayResult
	displayResult := ctx.ToDisplayResult()

	// Verify results
	testutil.AssertEqual(t, 1, len(displayResult.Packs))
	pack1 := displayResult.Packs[0]
	testutil.AssertEqual(t, 6, len(pack1.Files))

	// Create map for easy lookup
	fileMap := make(map[string]*types.DisplayFile)
	for i := range pack1.Files {
		key := pack1.Files[i].Handler + "_" + filepath.Base(pack1.Files[i].Path)
		fileMap[key] = &pack1.Files[i]
	}

	// Test symlink success message
	symlinkSuccess := fileMap["symlink_.vimrc"]
	testutil.AssertTrue(t, symlinkSuccess != nil, "symlink success file should exist")
	testutil.AssertEqual(t, "linked to $HOME/.vimrc", symlinkSuccess.Message)

	// Test symlink pending message
	symlinkPending := fileMap["symlink_.bashrc"]
	testutil.AssertTrue(t, symlinkPending != nil, "symlink pending file should exist")
	testutil.AssertEqual(t, "will be linked to $HOME/.bashrc", symlinkPending.Message)

	// Test homebrew success message with date
	homebrewSuccess := fileMap["homebrew_Brewfile"]
	testutil.AssertTrue(t, homebrewSuccess != nil, "homebrew success file should exist")
	expectedDate := testTime.Format("2006-01-02")
	testutil.AssertEqual(t, fmt.Sprintf("executed on %s", expectedDate), homebrewSuccess.Message)

	// Test shell_profile success message
	shellSuccess := fileMap["shell_profile_aliases.sh"]
	testutil.AssertTrue(t, shellSuccess != nil, "shell_profile success file should exist")
	testutil.AssertEqual(t, "included in shell profile", shellSuccess.Message)

	// Test install error message
	installError := fileMap["install_setup.sh"]
	testutil.AssertTrue(t, installError != nil, "install error file should exist")
	testutil.AssertEqual(t, "installation failed", installError.Message)

	// Test path pending message
	pathPending := fileMap["path_bin"]
	testutil.AssertTrue(t, pathPending != nil, "path pending file should exist")
	testutil.AssertEqual(t, "bin to be added to $PATH", pathPending.Message)
}
