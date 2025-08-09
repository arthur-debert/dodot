#!/usr/bin/env bats
# Minimal test for symlink power-up - happy path only

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

@test "symlink: deploy single gitconfig file" {
    # Deploy git pack
    run dodot deploy git
    [ "$status" -eq 0 ]
    
    # Verify symlink was created correctly
    assert_symlink_deployed "git" "gitconfig" "$HOME/gitconfig"
}