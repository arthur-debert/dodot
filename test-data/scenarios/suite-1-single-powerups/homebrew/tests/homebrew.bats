#!/usr/bin/env bats
# Minimal test for homebrew power-up - happy path only

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh
source /workspace/test-data/lib/assertions_homebrew.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "homebrew: YES - Brewfile processed (sentinel exists)" {
    # Run install command on brew pack
    run dodot install brew
    [ "$status" -eq 0 ]
    
    # Verify Brewfile was processed (sentinel created)
    assert_brewfile_processed "brew"
}

@test "homebrew: NO - Brewfile not processed (verify absence)" {
    skip "Not implemented"
}