#!/usr/bin/env bats

# Suite 5: Complex Multi-Pack/Handler Edge Cases
# 
# This suite tests complex edge cases that involve interactions between multiple
# packs and handlers, including conflict resolution, dependency ordering,
# state recovery, and large-scale deployments.

# Load common test setup with debug support
source /workspace/live-testing/lib/common.sh

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
    grep -q "pack_name=pack-a" "$HOME/shared-config" || fail "File $HOME/shared-config should contain: pack_name=pack-a"
    
    # Try to deploy pack-b which has same target file
    dodot_run deploy pack-b
    [ "$status" -eq 0 ]  # Currently succeeds (overwrites)
    
    # Current behavior: dodot overwrites the file content in the deployed directory
    # The symlink path remains the same, but content is replaced
    [ -L "$HOME/shared-config" ]
    
    # The symlink now contains pack-b's content
    grep -q "pack_name=pack-b" "$HOME/shared-config" || fail "File $HOME/shared-config should contain: pack_name=pack-b"
    
    # pack-a's content has been overwritten - verify it's NOT there
    run grep -q "pack_name=pack-a" "$HOME/shared-config"
    [ "$status" -ne 0 ]
    
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
    assert_file_not_exists "$HOME/.local/tools-consumer/marker.txt"
    
    # Now deploy the provider pack first
    dodot_run deploy tools-provider
    [ "$status" -eq 0 ]
    
    # Verify the tool is now available
    assert_path_deployed "tools-provider" "bin"
    assert_symlink_deployed "tools-provider" "bin/essential-tool" "$HOME/essential-tool"
    
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
    grep -q "installed-with-dependencies" "$HOME/.local/tools-consumer/marker.txt" || fail "File $HOME/.local/tools-consumer/marker.txt should contain: installed-with-dependencies"
    
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
    assert_symlink_not_deployed "$HOME/file2"  # Missing
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
    grep -q "file1_content=deployed" "$HOME/file1" || fail "File $HOME/file1 should contain: file1_content=deployed"
    grep -q "file2_content=deployed" "$HOME/file2" || fail "File $HOME/file2 should contain: file2_content=deployed"
    grep -q "file3_content=deployed" "$HOME/file3" || fail "File $HOME/file3 should contain: file3_content=deployed"
    
    # This test documents that dodot is idempotent and can restore
    # missing symlinks when re-run after partial state corruption
}

@test "large scale: 10+ packs with mixed handlers" {
    # Generate 12 test packs with various handler combinations
    local setup_script="/workspace/live-testing/scenarios/suite-5-complex-edge-cases/setup-large-scale.sh"
    "$setup_script" "$DOTFILES_ROOT"
    
    # Deploy all symlink-only packs (1-3)
    dodot_run deploy pack-1 pack-2 pack-3
    [ "$status" -eq 0 ]
    
    # Verify symlinks
    for i in 1 2 3; do
        assert_symlink_deployed "pack-$i" "config-$i" "$HOME/config-$i"
        grep -q "pack_id=$i" "$HOME/config-$i" || fail "File $HOME/config-$i should contain: pack_id=$i"
    done
    
    # Deploy path packs (4-5)
    dodot_run deploy pack-4 pack-5
    [ "$status" -eq 0 ]
    
    # Verify path deployments
    assert_path_deployed "pack-4" "bin"
    assert_path_deployed "pack-5" "bin"
    assert_file_executable "$HOME/tool-4"
    assert_file_executable "$HOME/tool-5"
    
    # Deploy shell profile packs (6-7)
    dodot_run deploy pack-6 pack-7
    [ "$status" -eq 0 ]
    
    # Verify profiles
    assert_profile_in_init "pack-6" "profile.sh"
    assert_profile_in_init "pack-7" "profile.sh"
    
    # Deploy template packs (8-9)
    dodot_run deploy pack-8 pack-9
    [ "$status" -eq 0 ]
    
    # Template packs no longer processed (template functionality removed)
    # Previously would have created config files from templates
    
    # Install pack with install script (10)
    dodot_run install pack-10
    [ "$status" -eq 0 ]
    
    assert_install_script_executed "pack-10"
    assert_install_artifact_exists "$HOME/.local/pack-10/marker.txt"
    
    # Deploy mixed packs (11-12)
    dodot_run deploy pack-11
    [ "$status" -eq 0 ]
    
    assert_symlink_deployed "pack-11" "settings" "$HOME/settings"
    assert_path_deployed "pack-11" "bin"
    assert_file_executable "$HOME/mixed-tool"
    
    # Install everything pack (12)
    dodot_run install pack-12
    [ "$status" -eq 0 ]
    
    assert_install_script_executed "pack-12"
    assert_file_exists "$HOME/.local/pack-12/install-time.txt"
    
    # Deploy remaining handlers for pack-12
    dodot_run deploy pack-12
    [ "$status" -eq 0 ]
    
    assert_symlink_deployed "pack-12" "complete-config" "$HOME/complete-config"
    assert_path_deployed "pack-12" "bin"
    assert_profile_in_init "pack-12" "profile.sh"
    # Template functionality removed - data file would not be created
    
    # Verify system handles 12 packs with mixed handlers correctly
    # This test documents dodot's ability to scale to many packs
}