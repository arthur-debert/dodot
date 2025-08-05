package status

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestStatusPacks(t *testing.T) {
	// Create paths instance for testing
	tempDir := testutil.TempDir(t, "status-packs-test")
	pathsInstance, err := paths.New(tempDir)
	testutil.AssertNoError(t, err)

	// Clean up any existing sentinel files before running tests
	_ = os.RemoveAll(pathsInstance.InstallDir())
	_ = os.RemoveAll(pathsInstance.HomebrewDir())
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
				testutil.AssertEqual(t, "Install script not yet executed", installStatus.Description)
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
				sentinelPath := filepath.Join(pathsInstance.InstallDir(), "test-pack")
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
			name: "status of pack with changed install script",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "test-pack")

				// Create sentinel file with old checksum
				sentinelPath := filepath.Join(pathsInstance.InstallDir(), "test-pack")
				sentinelDir := filepath.Dir(sentinelPath)
				err := os.MkdirAll(sentinelDir, 0755)
				if err != nil {
					t.Fatal(err)
				}
				// Write an old checksum
				err = os.WriteFile(sentinelPath, []byte("old-checksum-value"), 0644)
				if err != nil {
					t.Fatal(err)
				}
				// Set modification time to make it look like it was installed
				err = os.Chtimes(sentinelPath, time.Now().Add(-24*time.Hour), time.Now().Add(-24*time.Hour))
				if err != nil {
					t.Fatal(err)
				}

				// Now create install script with different content
				scriptContent := "#!/bin/bash\necho 'new content'"
				testutil.CreateFile(t, pack, "install.sh", scriptContent)

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
				testutil.AssertEqual(t, "Changed", installStatus.State)
				testutil.AssertContains(t, installStatus.Description, "script has changed since execution")
			},
		},
		{
			name: "status of pack with brewfile",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				testutil.CreateFile(t, pack, "Brewfile", "brew 'git'\nbrew 'tmux'")
				return root, []string{"test-pack"}
			},
			validate: func(t *testing.T, result *types.PackStatusResult) {
				testutil.AssertEqual(t, 1, len(result.Packs))
				pack := result.Packs[0]

				var homebrewStatus *types.PowerUpStatus
				for i := range pack.PowerUpState {
					if pack.PowerUpState[i].Name == "homebrew" {
						homebrewStatus = &pack.PowerUpState[i]
						break
					}
				}
				testutil.AssertNotNil(t, homebrewStatus)
				testutil.AssertEqual(t, "Not Installed", homebrewStatus.State)
				testutil.AssertEqual(t, "Brewfile not yet executed", homebrewStatus.Description)
			},
		},
		{
			name: "status of pack with executed homebrew",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				brewfileContent := "brew 'git'\nbrew 'tmux'"
				testutil.CreateFile(t, pack, "Brewfile", brewfileContent)

				// Calculate checksum
				brewfilePath := filepath.Join(pack, "Brewfile")
				checksum, err := testutil.CalculateFileChecksum(brewfilePath)
				if err != nil {
					t.Fatal(err)
				}

				// Create sentinel file
				sentinelPath := filepath.Join(pathsInstance.HomebrewDir(), "test-pack")
				sentinelDir := filepath.Dir(sentinelPath)
				err = os.MkdirAll(sentinelDir, 0755)
				if err != nil {
					t.Fatal(err)
				}
				err = os.WriteFile(sentinelPath, []byte(checksum), 0644)
				if err != nil {
					t.Fatal(err)
				}

				return root, []string{"test-pack"}
			},
			validate: func(t *testing.T, result *types.PackStatusResult) {
				testutil.AssertEqual(t, 1, len(result.Packs))
				pack := result.Packs[0]

				var brewfileStatus *types.PowerUpStatus
				for i := range pack.PowerUpState {
					if pack.PowerUpState[i].Name == "homebrew" {
						brewfileStatus = &pack.PowerUpState[i]
						break
					}
				}
				testutil.AssertNotNil(t, brewfileStatus)
				testutil.AssertEqual(t, "Installed", brewfileStatus.State)
				testutil.AssertContains(t, brewfileStatus.Description, "Installed on")
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

				// Should have symlink status (even though it's "Unknown")
				var symlinkStatus *types.PowerUpStatus
				for i := range pack.PowerUpState {
					if pack.PowerUpState[i].Name == "symlink" {
						symlinkStatus = &pack.PowerUpState[i]
						break
					}
				}
				testutil.AssertNotNil(t, symlinkStatus)
				testutil.AssertEqual(t, "Unknown", symlinkStatus.State)
			},
		},
		{
			name: "status of all packs when no pack names provided",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				// Create several packs
				for _, packName := range []string{"vim", "zsh", "git", "tmux"} {
					pack := testutil.CreateDir(t, root, packName)
					// Add a file to make it a valid pack
					testutil.CreateFile(t, pack, "config", "# "+packName+" config")
				}
				return root, nil // nil or empty means all packs
			},
			validate: func(t *testing.T, result *types.PackStatusResult) {
				testutil.AssertEqual(t, 4, len(result.Packs))

				// Verify all packs are included
				packNames := make(map[string]bool)
				for _, p := range result.Packs {
					packNames[p.Name] = true
				}
				for _, expected := range []string{"vim", "zsh", "git", "tmux"} {
					testutil.AssertTrue(t, packNames[expected], "Expected pack %s to be in results", expected)
				}
			},
		},
		{
			name: "status respects .dodotignore",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				// Create normal pack
				pack1 := testutil.CreateDir(t, root, "normal-pack")
				testutil.CreateFile(t, pack1, "config", "# config")

				// Create ignored pack
				pack2 := testutil.CreateDir(t, root, "ignored-pack")
				testutil.CreateFile(t, pack2, ".dodotignore", "")
				testutil.CreateFile(t, pack2, "config", "# config")

				return root, nil // Check all packs
			},
			validate: func(t *testing.T, result *types.PackStatusResult) {
				testutil.AssertEqual(t, 1, len(result.Packs))
				testutil.AssertEqual(t, "normal-pack", result.Packs[0].Name)
			},
		},
		{
			name: "status includes all powerup types",
			setup: func(t *testing.T) (string, []string) {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "full-pack")
				// Create files that would trigger different powerups
				testutil.CreateFile(t, pack, "install.sh", "#!/bin/bash\necho install")
				testutil.CreateFile(t, pack, "Brewfile", "brew 'git'")
				testutil.CreateFile(t, pack, ".vimrc", "\" vim config")
				testutil.CreateFile(t, pack, "aliases.sh", "# aliases")
				testutil.CreateDir(t, pack, "bin")

				return root, []string{"full-pack"}
			},
			validate: func(t *testing.T, result *types.PackStatusResult) {
				testutil.AssertEqual(t, 1, len(result.Packs))
				pack := result.Packs[0]

				// Check that we have status for multiple powerup types
				powerupTypes := make(map[string]bool)
				for _, status := range pack.PowerUpState {
					powerupTypes[status.Name] = true
				}

				// Should have at least install, brewfile, and symlink
				testutil.AssertTrue(t, powerupTypes["install"], "Expected install status")
				testutil.AssertTrue(t, powerupTypes["homebrew"], "Expected brewfile status")
				testutil.AssertTrue(t, powerupTypes["symlink"], "Expected symlink status")
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

func TestGetRunOnceStatus(t *testing.T) {
	// Create paths instance for testing
	tempDir := testutil.TempDir(t, "status-test")
	pathsInstance, err := paths.New(tempDir)
	testutil.AssertNoError(t, err)

	// Clean up any existing sentinel files
	_ = os.RemoveAll(pathsInstance.InstallDir())
	_ = os.RemoveAll(pathsInstance.HomebrewDir())

	tests := []struct {
		name     string
		setup    func(t *testing.T) string // returns pack path
		powerup  string
		validate func(t *testing.T, status *core.RunOnceStatus)
		wantNil  bool
	}{
		{
			name: "no install script",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				return pack
			},
			powerup: "install",
			wantNil: true,
		},
		{
			name: "install script not executed",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				testutil.CreateFile(t, pack, "install.sh", "#!/bin/bash\necho test")
				return pack
			},
			powerup: "install",
			validate: func(t *testing.T, status *core.RunOnceStatus) {
				testutil.AssertFalse(t, status.Executed)
				testutil.AssertFalse(t, status.Changed)
				testutil.AssertNotEqual(t, "", status.Checksum)
			},
		},
		{
			name: "install script executed and unchanged",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				packName := filepath.Base(pack)

				scriptContent := "#!/bin/bash\necho test"
				scriptPath := filepath.Join(pack, "install.sh")
				testutil.CreateFile(t, pack, "install.sh", scriptContent)

				// Calculate checksum
				checksum, err := testutil.CalculateFileChecksum(scriptPath)
				if err != nil {
					t.Fatal(err)
				}

				// Create sentinel
				sentinelPath := filepath.Join(pathsInstance.InstallDir(), packName)
				err = os.MkdirAll(filepath.Dir(sentinelPath), 0755)
				if err != nil {
					t.Fatal(err)
				}
				err = os.WriteFile(sentinelPath, []byte(checksum), 0644)
				if err != nil {
					t.Fatal(err)
				}

				return pack
			},
			powerup: "install",
			validate: func(t *testing.T, status *core.RunOnceStatus) {
				testutil.AssertTrue(t, status.Executed)
				testutil.AssertFalse(t, status.Changed)
				testutil.AssertNotEqual(t, time.Time{}, status.ExecutedAt)
			},
		},
		{
			name: "brewfile status",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "status-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				testutil.CreateFile(t, pack, "Brewfile", "brew 'git'")
				return pack
			},
			powerup: "homebrew",
			validate: func(t *testing.T, status *core.RunOnceStatus) {
				testutil.AssertFalse(t, status.Executed)
				testutil.AssertNotEqual(t, "", status.Checksum)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			packPath := tt.setup(t)

			status, err := core.GetRunOnceStatus(packPath, tt.powerup, pathsInstance)
			testutil.AssertNoError(t, err)

			if tt.wantNil {
				testutil.AssertNil(t, status)
				return
			}

			testutil.AssertNotNil(t, status)
			if tt.validate != nil {
				tt.validate(t, status)
			}
		})
	}
}
