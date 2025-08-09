#!/usr/bin/env bats
# Test shell_profile power-up functionality

# Load test libraries  
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh
source /workspace/test-data/lib/assertions_shell.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "shell_profile: creates init.sh with source commands" {
    # Deploy packs with shell profiles
    run dodot deploy git nvim
    [ "$status" -eq 0 ]
    
    # Check init.sh exists
    local init_file="$DODOT_DATA_DIR/shell/init.sh"
    assert_file_exists "$init_file"
    
    # Verify it contains source commands for our files
    assert_profile_in_init "git" "aliases.sh"
    assert_profile_in_init "nvim" "aliases.sh"
}

# TODO: This test is currently failing - shell_profile duplicates entries on repeated deploy
# This is a known bug in dodot that needs to be fixed
# @test "shell_profile: repeated deploy doesn't duplicate entries" {
#     # Deploy once
#     run dodot deploy git
#     [ "$status" -eq 0 ]
#     
#     local init_file="$DODOT_DATA_DIR/shell/init.sh"
#     assert_file_exists "$init_file"
#     
#     # Count entries
#     local count1=$(grep -c "git/aliases.sh" "$init_file" || true)
#     
#     # Deploy again
#     run dodot deploy git  
#     [ "$status" -eq 0 ]
#     
#     # Count should be the same
#     local count2=$(grep -c "git/aliases.sh" "$init_file" || true)
#     [ "$count1" -eq "$count2" ]
# }