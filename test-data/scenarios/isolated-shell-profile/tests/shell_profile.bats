#!/usr/bin/env bats
# Minimal test for shell_profile power-up - happy path only

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

@test "shell_profile: deploy creates init.sh with source command" {
    # Deploy nvim pack
    run dodot deploy nvim
    [ "$status" -eq 0 ]
    
    # Verify init.sh was created and contains the profile entry
    assert_profile_in_init "nvim" "profile.sh"
}