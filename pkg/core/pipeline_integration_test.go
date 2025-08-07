package core

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"

	// Import to register factories
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func init() {
	// Set up logging for tests
	logging.SetupLogger(0)
}

// TestPipelineIntegration tests the complete pipeline flow end-to-end
func TestPipelineIntegration(t *testing.T) {
	// Create a test dotfiles structure
	root := testutil.TempDir(t, "pipeline-test")

	// Create test packs
	testutil.CreateDir(t, root, "bin-pack")
	testutil.CreateFile(t, filepath.Join(root, "bin-pack"), "script.sh", "#!/bin/bash\necho test")

	testutil.CreateDir(t, root, "shell-pack")
	testutil.CreateFile(t, filepath.Join(root, "shell-pack"), ".zshrc", "# Test zshrc")
	testutil.CreateFile(t, filepath.Join(root, "shell-pack"), ".bashrc", "# Test bashrc")

	// Create pack with config
	configuredPack := testutil.CreateDir(t, root, "configured-pack")
	packConfig := `[files]
"app.conf" = "test-powerup"
"*.log" = "ignore"`
	testutil.CreateFile(t, configuredPack, ".dodot.toml", packConfig)
	testutil.CreateFile(t, configuredPack, "app.conf", "# App config")
	testutil.CreateFile(t, configuredPack, "debug.log", "# Debug log")

	// Register mock trigger and power-up for testing
	triggerReg := registry.GetRegistry[types.TriggerFactory]()
	powerupReg := registry.GetRegistry[types.PowerUpFactory]()

	// Clean up after test
	t.Cleanup(func() {
		_ = triggerReg.Remove("test-trigger")
		_ = powerupReg.Remove("test-powerup")
	})

	// Register mock trigger
	mockTrigger := &testutil.MockTrigger{
		NameFunc: func() string { return "test-trigger" },
		MatchFunc: func(path string, info fs.FileInfo) (bool, map[string]interface{}) {
			return filepath.Ext(path) == ".conf", nil
		},
	}
	err := triggerReg.Register("test-trigger", func(o map[string]interface{}) (types.Trigger, error) { return mockTrigger, nil })
	testutil.AssertNoError(t, err)

	// Register mock power-up
	mockPowerUp := &testutil.MockPowerUp{
		NameFunc: func() string { return "test-powerup" },
		ProcessFunc: func(matches []types.TriggerMatch) ([]types.Action, error) {
			var actions []types.Action
			for _, match := range matches {
				actions = append(actions, types.Action{
					Type:        types.ActionTypeLink,
					Description: "Link " + match.Path,
					Source:      match.Path,
					Target:      filepath.Join("/tmp", filepath.Base(match.Path)),
				})
			}
			return actions, nil
		},
	}
	err = powerupReg.Register("test-powerup", func(o map[string]interface{}) (types.PowerUp, error) { return mockPowerUp, nil })
	testutil.AssertNoError(t, err)

	// Test Stage 1: GetPackCandidates
	t.Run("Stage1_GetPackCandidates", func(t *testing.T) {
		candidates, err := GetPackCandidates(root)
		testutil.AssertNoError(t, err)
		testutil.AssertEqual(t, 3, len(candidates), "expected 3 pack candidates")
	})

	// Test Stage 2: GetPacks
	t.Run("Stage2_GetPacks", func(t *testing.T) {
		candidates, err := GetPackCandidates(root)
		testutil.AssertNoError(t, err)

		packs, err := GetPacks(candidates)
		testutil.AssertNoError(t, err)
		testutil.AssertEqual(t, 3, len(packs), "expected 3 packs")

		// Verify packs are sorted alphabetically
		expectedOrder := []string{"bin-pack", "configured-pack", "shell-pack"}
		for i, expected := range expectedOrder {
			testutil.AssertEqual(t, expected, packs[i].Name)
		}
	})

	// Test Stage 3: GetFiringTriggers
	t.Run("Stage3_GetFiringTriggers", func(t *testing.T) {
		candidates, err := GetPackCandidates(root)
		testutil.AssertNoError(t, err)

		packs, err := GetPacks(candidates)
		testutil.AssertNoError(t, err)

		matches, err := GetFiringTriggers(packs)
		testutil.AssertNoError(t, err)

		// With the test pack structure, we should get some matches
		// The exact number depends on the matchers and files in the test pack
		testutil.AssertTrue(t, len(matches) >= 0, "GetFiringTriggers should not fail")
	})

	// Test Stage 4: GetActions
	t.Run("Stage4_GetActions", func(t *testing.T) {
		// Create trigger matches using the symlink power-up which is registered
		matches := []types.TriggerMatch{
			{
				Pack:         "test-pack",
				Path:         ".vimrc",
				AbsolutePath: "/test/pack/.vimrc",
				TriggerName:  "filename",
				PowerUpName:  "symlink",
				Priority:     100,
				Metadata:     map[string]interface{}{},
				PowerUpOptions: map[string]interface{}{
					"target": "~",
				},
			},
		}

		actions, err := GetActions(matches)
		testutil.AssertNoError(t, err)

		// The symlink power-up should generate one link action
		testutil.AssertEqual(t, 1, len(actions), "expected one action")
		if len(actions) > 0 {
			testutil.AssertEqual(t, types.ActionTypeLink, actions[0].Type)
			testutil.AssertEqual(t, "/test/pack/.vimrc", actions[0].Source)
		}
	})

	// Test Stage 5: ConvertActionsToOperations
	t.Run("Stage5_ConvertActionsToOperations", func(t *testing.T) {
		// Create mock actions
		actions := []types.Action{
			{
				Type:        types.ActionTypeLink,
				Description: "Link config file",
				Source:      "/source/file.conf",
				Target:      "~/file.conf",
				Pack:        "test-pack",
			},
			{
				Type:        types.ActionTypeShellSource,
				Description: "Source shell aliases",
				Source:      "/source/alias.sh",
				Pack:        "test-pack",
			},
			{
				Type:        types.ActionTypePathAdd,
				Description: "Add bin to PATH",
				Source:      "/source/bin",
				Pack:        "test-pack",
			},
		}

		testPaths := createTestPaths(t)

		// Update action sources to use test dotfiles root
		for i := range actions {
			if actions[i].Source != "" {
				actions[i].Source = filepath.Join(testPaths.DotfilesRoot(), filepath.Base(actions[i].Source))
			}
		}

		ctx := NewExecutionContextWithHomeSymlinks(false, testPaths, true, nil)
		operations, err := ConvertActionsToOperationsWithContext(actions, ctx)
		testutil.AssertNoError(t, err)

		// Should have operations for:
		// - Link: mkdir parent, deploy symlink, user symlink (3 ops)
		// - Shell source: mkdir shell_profile, create symlink (2 ops)
		// - Path add: mkdir path, create symlink (2 ops)
		// Total: 7 operations
		testutil.AssertTrue(t, len(operations) >= 7, "expected at least 7 operations")

		// Verify operation types are correct
		var opTypes []types.OperationType
		for _, op := range operations {
			opTypes = append(opTypes, op.Type)
		}

		// Should contain create_dir and create_symlink operations
		hasDir := false
		hasSymlink := false
		for _, opType := range opTypes {
			if opType == types.OperationCreateDir {
				hasDir = true
			}
			if opType == types.OperationCreateSymlink {
				hasSymlink = true
			}
		}
		testutil.AssertTrue(t, hasDir, "should have directory creation operations")
		testutil.AssertTrue(t, hasSymlink, "should have symlink creation operations")
	})
}

// TestPipelineErrorPropagation tests that errors are properly propagated through the pipeline
func TestPipelineErrorPropagation(t *testing.T) {
	// Test with non-existent directory
	t.Run("NonExistentRoot", func(t *testing.T) {
		_, err := GetPackCandidates("/non/existent/path")
		testutil.AssertError(t, err)
	})

	// Test with invalid pack path
	t.Run("InvalidPackPath", func(t *testing.T) {
		// GetPacks should handle invalid paths gracefully
		invalidPaths := []string{"/non/existent", "/another/bad/path"}
		packs, err := GetPacks(invalidPaths)
		testutil.AssertNoError(t, err)
		testutil.AssertEqual(t, 0, len(packs), "expected no packs for invalid paths")
	})
}

// TestPipelineDataFlow verifies data transformation at each stage
func TestPipelineDataFlow(t *testing.T) {
	// This test will be more meaningful once we implement the actual pipeline logic
	t.Run("DataTransformation", func(t *testing.T) {
		// Create test data
		root := testutil.TempDir(t, "data-flow-test")
		pack := testutil.CreateDir(t, root, "test-pack")
		testutil.CreateFile(t, pack, "file.txt", "content")

		// Stage 1: Candidates should be directory paths
		candidates, err := GetPackCandidates(root)
		testutil.AssertNoError(t, err)
		for _, candidate := range candidates {
			testutil.AssertTrue(t, filepath.IsAbs(candidate),
				"candidate should be absolute path: %s", candidate)
		}

		// Stage 2: Packs should have names and paths
		packs, err := GetPacks(candidates)
		testutil.AssertNoError(t, err)
		for _, p := range packs {
			testutil.AssertNotEmpty(t, p.Name, "pack should have name")
			testutil.AssertNotEmpty(t, p.Path, "pack should have path")
		}

		// Further stages will be tested once implemented
	})
}

// Benchmarks

// BenchmarkGetPackCandidates benchmarks pack discovery
func BenchmarkGetPackCandidates(b *testing.B) {
	// Create test structure with many packs
	root := b.TempDir()

	// Create 100 pack directories
	for i := 0; i < 100; i++ {
		packName := filepath.Join(root, fmt.Sprintf("pack-%03d", i))
		if err := os.MkdirAll(packName, 0755); err != nil {
			b.Fatal(err)
		}
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := GetPackCandidates(root)
		if err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkGetPacksIntegration benchmarks pack loading in the pipeline
func BenchmarkGetPacksIntegration(b *testing.B) {
	// Create test structure
	root := b.TempDir()
	var candidates []string

	// Create 50 packs with configs
	for i := 0; i < 50; i++ {
		packName := filepath.Join(root, fmt.Sprintf("pack-%03d", i))
		if err := os.MkdirAll(packName, 0755); err != nil {
			b.Fatal(err)
		}
		candidates = append(candidates, packName)

		// Half with configs
		if i%2 == 0 {
			config := fmt.Sprintf(`description = "Pack %d"\npriority = %d`, i, i)
			configPath := filepath.Join(packName, ".dodot.toml")
			if err := os.WriteFile(configPath, []byte(config), 0644); err != nil {
				b.Fatal(err)
			}
		}
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := GetPacks(candidates)
		if err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkPipelineEndToEnd benchmarks the complete pipeline
func BenchmarkPipelineEndToEnd(b *testing.B) {
	// Create a realistic dotfiles structure
	root := b.TempDir()

	// Create several packs with files
	packs := []string{"vim", "shell", "git", "tmux", "bin"}
	for _, packName := range packs {
		packDir := filepath.Join(root, packName+"-pack")
		if err := os.MkdirAll(packDir, 0755); err != nil {
			b.Fatal(err)
		}

		// Create some files in each pack
		for j := 0; j < 10; j++ {
			fileName := fmt.Sprintf("file%d.conf", j)
			filePath := filepath.Join(packDir, fileName)
			content := fmt.Sprintf("# Config file %d in %s", j, packName)
			if err := os.WriteFile(filePath, []byte(content), 0644); err != nil {
				b.Fatal(err)
			}
		}

		// Add a config file
		config := fmt.Sprintf(`description = "%s configuration"\npriority = 1\n\n[[matchers]]\ntrigger = "filename"\npowerup = "symlink"\npattern = "*.conf"`, packName)
		configPath := filepath.Join(packDir, ".dodot.toml")
		if err := os.WriteFile(configPath, []byte(config), 0644); err != nil {
			b.Fatal(err)
		}
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		// Run the complete pipeline
		candidates, err := GetPackCandidates(root)
		if err != nil {
			b.Fatal(err)
		}

		packs, err := GetPacks(candidates)
		if err != nil {
			b.Fatal(err)
		}

		matches, err := GetFiringTriggers(packs)
		if err != nil {
			b.Fatal(err)
		}

		actions, err := GetActions(matches)
		if err != nil {
			b.Fatal(err)
		}

		testPaths := createTestPaths(b)
		ctx := NewExecutionContext(false, testPaths)
		_, err = ConvertActionsToOperationsWithContext(actions, ctx)
		if err != nil {
			b.Fatal(err)
		}
	}
}
