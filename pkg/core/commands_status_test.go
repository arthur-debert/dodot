package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestStatusPacks(t *testing.T) {
	// Clean up any existing sentinel files before running tests
	_ = os.RemoveAll(paths.GetInstallDir())
	_ = os.RemoveAll(paths.GetBrewfileDir())
	tests := []struct {
		name      string
		setup     func(t *testing.T) (string, []string)
		packNames []string
		validate  func(t *testing.T, result *types.PackStatusResult)
		wantErr   bool
	}{
		{
			name: "status of single pack with install script",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				testutil.CreateFile(t, pack, "install.sh", "#!/bin/bash\necho test")
				return root, []string{"test-pack"}
			},
			validate: func(t *testing.T, result *types.PackStatusResult) {
				testutil.AssertEqual(t, 1, len(result.Packs))
				pack := result.Packs[0]
				testutil.AssertEqual(t, "test-pack", pack.Name)

				// Should have status for install power-up
				var installStatus *types.PowerUpStatus
				for i := range pack.PowerUpState {
					if pack.PowerUpState[i].Name == "install" {
						installStatus = &pack.PowerUpState[i]
						break
					}
				}
				testutil.AssertNotNil(t, installStatus)
				testutil.AssertEqual(t, "Not Installed", installStatus.State)
			},
		},
		{
			name: "status of pack with executed install script",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				scriptContent := "#!/bin/bash\necho test"
				testutil.CreateFile(t, pack, "install.sh", scriptContent)

				// Calculate the actual checksum
				scriptPath := filepath.Join(pack, "install.sh")
				checksum, err := testutil.CalculateFileChecksum(scriptPath)
				if err != nil {
					t.Fatal(err)
				}

				// Create sentinel file to simulate executed install
				sentinelPath := filepath.Join(paths.GetInstallDir(), "test-pack")
				sentinelDir := filepath.Dir(sentinelPath)
				err = os.MkdirAll(sentinelDir, 0755)
				if err != nil {
					t.Fatal(err)
				}
				// Sentinel file contains just the checksum
				err = os.WriteFile(sentinelPath, []byte(checksum), 0644)
				if err != nil {
					t.Fatal(err)
				}

				return root, []string{"test-pack"}
			},
			validate: func(t *testing.T, result *types.PackStatusResult) {
				testutil.AssertEqual(t, 1, len(result.Packs))
				pack := result.Packs[0]

				var installStatus *types.PowerUpStatus
				for i := range pack.PowerUpState {
					if pack.PowerUpState[i].Name == "install" {
						installStatus = &pack.PowerUpState[i]
						break
					}
				}
				testutil.AssertNotNil(t, installStatus)
				testutil.AssertEqual(t, "Installed", installStatus.State)
				testutil.AssertContains(t, installStatus.Description, "Installed on")
			},
		},
		{
			name: "status of multiple packs",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				pack1 := testutil.CreateDir(t, root, "pack1")
				pack2 := testutil.CreateDir(t, root, "pack2")
				testutil.CreateFile(t, pack1, "install.sh", "#!/bin/bash\necho pack1")
				testutil.CreateFile(t, pack2, "Brewfile", "brew 'git'")
				return root, nil // Check all packs
			},
			validate: func(t *testing.T, result *types.PackStatusResult) {
				testutil.AssertEqual(t, 2, len(result.Packs))

				// Verify we got both packs
				packNames := make(map[string]bool)
				for _, p := range result.Packs {
					packNames[p.Name] = true
				}
				testutil.AssertTrue(t, packNames["pack1"])
				testutil.AssertTrue(t, packNames["pack2"])
			},
		},
		{
			name: "status with non-existent pack",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				testutil.CreateDir(t, root, "real-pack")
				return root, []string{"real-pack", "fake-pack"}
			},
			wantErr: true,
		},
		{
			name: "empty pack status",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				testutil.CreateDir(t, root, "empty-pack")
				return root, []string{"empty-pack"}
			},
			validate: func(t *testing.T, result *types.PackStatusResult) {
				testutil.AssertEqual(t, 1, len(result.Packs))
				pack := result.Packs[0]
				testutil.AssertEqual(t, "empty-pack", pack.Name)
				// Should still have some power-up states even if not applicable
				testutil.AssertTrue(t, len(pack.PowerUpState) > 0)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			root, packNames := tt.setup(t)

			opts := StatusPacksOptions{
				DotfilesRoot: root,
				PackNames:    packNames,
			}

			result, err := StatusPacks(opts)

			if tt.wantErr {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertNotNil(t, result)

			if tt.validate != nil {
				tt.validate(t, result)
			}
		})
	}
}
