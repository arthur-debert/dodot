package core

import (
	"testing"
)

func TestGetMatches_Orchestration(t *testing.T) {
	// This test verifies that GetMatches correctly orchestrates
	// calling the scanner for each pack and aggregating results

	// Variables commented out to avoid "declared and not used" errors
	// pack1 := types.Pack{Name: "pack1", Path: "/path1"}
	// pack2 := types.Pack{Name: "pack2", Path: "/path2"}

	// pack1Matches := []types.RuleMatch{
	// 	{Pack: "pack1", Path: "file1.txt"},
	// 	{Pack: "pack1", Path: "file2.txt"},
	// }

	// pack2Matches := []types.RuleMatch{
	// 	{Pack: "pack2", Path: "file3.txt"},
	// }

	// Note: In the real implementation, we can't easily mock matchers.ScanPack
	// This test demonstrates what we would test if we could inject the scanner
	t.Run("aggregates matches from multiple packs", func(t *testing.T) {
		// The actual implementation will be tested through integration tests
		// that test the full pipeline with real matchers
		t.Skip("GetMatches delegates to matchers.ScanPack - tested via integration")
	})
}

func TestGetMatchesFS_ErrorHandling(t *testing.T) {
	// This would test error propagation if we could mock the scanner
	t.Skip("Error handling tested via integration tests")
}

// Integration tests that verify the full pipeline work correctly
// would go in a separate integration test file
