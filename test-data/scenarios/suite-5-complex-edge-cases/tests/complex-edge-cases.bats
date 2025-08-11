#!/usr/bin/env bats

# Suite 5: Complex Multi-Pack/Power-Up Edge Cases
# 
# This suite tests complex edge cases that involve interactions between multiple
# packs and power-ups, including conflict resolution, dependency ordering,
# state recovery, and large-scale deployments.

# Load common test setup with debug support
source /workspace/test-data/lib/common.sh

# Setup before all tests
setup() {
    setup_with_debug
}

# Cleanup after each test
teardown() {
    teardown_with_debug
}

@test "file conflicts: two packs symlink same target" {
    # Deploy pack-a first
    dodot_run deploy pack-a
    [ "$status" -eq 0 ]
    
    # Verify pack-a's symlink was created
    assert_symlink_deployed "pack-a" "shared-config" "$HOME/shared-config"
    assert_template_contains "$HOME/shared-config" "pack_name=pack-a"
    
    # Try to deploy pack-b which has same target file
    dodot_run deploy pack-b
    [ "$status" -eq 0 ]  # Currently succeeds (overwrites)
    
    # Current behavior: dodot overwrites the file content in the deployed directory
    # The symlink path remains the same, but content is replaced
    [ -L "$HOME/shared-config" ]
    
    # The symlink now contains pack-b's content
    assert_template_contains "$HOME/shared-config" "pack_name=pack-b"
    
    # pack-a's content has been overwritten
    ! grep -q "pack_name=pack-a" "$HOME/shared-config"
    
    # Verify both packs tried to deploy to the same symlink name
    assert_symlink_deployed "pack-b" "shared-config" "$HOME/shared-config"
    
    # This documents current behavior: last deployed pack wins in conflicts
    # dodot overwrites the deployed file content when conflicts occur
}

@test "dependency order: pack A depends on pack B" {
    # Try to install tools-consumer first (should fail due to missing dependency)
    dodot_run install tools-consumer
    [ "$status" -ne 0 ]  # Should fail
    
    # Error should mention the missing tool
    [[ "$output" == *"essential-tool"* ]] || [[ "$output" == *"not found"* ]]
    
    # Verify install did not complete
    assert_install_script_not_executed "tools-consumer"
    [ ! -f "$HOME/.local/tools-consumer/marker.txt" ]
    
    # Now deploy the provider pack first
    dodot_run deploy tools-provider
    [ "$status" -eq 0 ]
    
    # Verify the tool is now available
    assert_path_deployed "tools-provider" "bin"
    [ -L "$HOME/essential-tool" ]
    
    # Test the tool works
    run "$HOME/essential-tool"
    [ "$status" -eq 0 ]
    [[ "$output" == *"Essential tool v1.0"* ]]
    
    # Now install tools-consumer (should succeed)
    dodot_run install tools-consumer
    [ "$status" -eq 0 ]
    
    # Verify installation completed successfully
    assert_install_script_executed "tools-consumer"
    assert_install_artifact_exists "$HOME/.local/tools-consumer/marker.txt"
    grep -q "installed-with-dependencies" "$HOME/.local/tools-consumer/marker.txt"
    
    # Verify consumer config was deployed
    assert_symlink_deployed "tools-consumer" "consumer-config" "$HOME/consumer-config"
    
    # This test documents that packs can have dependencies on other packs
    # and deployment order matters for successful installation
}

@test "state corruption: recovery from partial deployment" {
    # First, do a normal deployment
    dodot_run deploy partial-deploy
    [ "$status" -eq 0 ]
    
    # Verify all files are deployed correctly
    assert_symlink_deployed "partial-deploy" "file1" "$HOME/file1"
    assert_symlink_deployed "partial-deploy" "file2" "$HOME/file2"
    assert_symlink_deployed "partial-deploy" "file3" "$HOME/file3"
    
    # Now simulate corruption by removing one symlink (as if deployment was interrupted)
    rm "$HOME/file2"
    
    # Verify partial state
    [ -L "$HOME/file1" ]
    [ ! -L "$HOME/file2" ]  # Missing
    [ -L "$HOME/file3" ]
    
    # Run deploy again - dodot should restore missing symlink
    dodot_run deploy partial-deploy
    [ "$status" -eq 0 ]
    
    # All files should be deployed again
    assert_symlink_deployed "partial-deploy" "file1" "$HOME/file1"
    assert_symlink_deployed "partial-deploy" "file2" "$HOME/file2"
    assert_symlink_deployed "partial-deploy" "file3" "$HOME/file3"
    
    # Run deploy multiple times - should be idempotent
    dodot_run deploy partial-deploy
    [ "$status" -eq 0 ]
    
    dodot_run deploy partial-deploy
    [ "$status" -eq 0 ]
    
    # Everything should still work after multiple runs
    assert_template_contains "$HOME/file1" "file1_content=deployed"
    assert_template_contains "$HOME/file2" "file2_content=deployed"
    assert_template_contains "$HOME/file3" "file3_content=deployed"
    
    # This test documents that dodot is idempotent and can restore
    # missing symlinks when re-run after partial state corruption
}

@test "large scale: 10+ packs with mixed power-ups" {
    skip "Not implemented"
    # Test case with many packs using different combinations of power-ups
    # Should handle complex deployments efficiently
}