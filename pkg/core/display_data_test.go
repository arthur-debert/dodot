package core

import (
	"path/filepath"
	"testing"

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
