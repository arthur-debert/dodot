#!/usr/bin/env bats
# Test symlink power-up functionality

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "symlink: deploy single file" {
    # Deploy git pack
    run dodot deploy git
    [ "$status" -eq 0 ]
    
    # Verify symlink was created
    assert_symlink_deployed "git" "gitconfig" "$HOME/gitconfig"
}

@test "symlink: deploy multiple files from different packs" {
    # Deploy both packs
    run dodot deploy git nvim
    [ "$status" -eq 0 ]
    
    # Verify both symlinks
    assert_symlink_deployed "git" "gitconfig" "$HOME/gitconfig"
    assert_symlink_deployed "nvim" "init.lua" "$HOME/init.lua"
}

@test "symlink: repeated deploy is idempotent" {
    # Deploy once
    run dodot deploy git
    [ "$status" -eq 0 ]
    assert_symlink_deployed "git" "gitconfig" "$HOME/gitconfig"
    
    # Deploy again - should not fail
    run dodot deploy git
    [ "$status" -eq 0 ]
    assert_symlink_deployed "git" "gitconfig" "$HOME/gitconfig"
}

@test "symlink: handles missing pack gracefully" {
    # Try to deploy non-existent pack
    run dodot deploy nonexistent
    [ "$status" -ne 0 ]
    [[ "$output" =~ "not found" ]]
}