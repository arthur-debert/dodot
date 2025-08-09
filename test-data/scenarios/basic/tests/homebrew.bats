#!/usr/bin/env bats
# Test homebrew power-up functionality

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

@test "homebrew: processes Brewfile on first install" {
    # Run install command
    run dodot install tools
    [ "$status" -eq 0 ]
    
    # Verify Brewfile was processed (sentinel created)
    assert_brewfile_processed "tools"
}

@test "homebrew: skips Brewfile on second install without force" {
    # First install
    run dodot install tools
    [ "$status" -eq 0 ]
    assert_brewfile_processed "tools"
    
    # Get original sentinel timestamp
    local sentinel="$DODOT_DATA_DIR/homebrew/tools"
    local orig_time=$(stat -c %Y "$sentinel" 2>/dev/null || stat -f %m "$sentinel")
    
    # Second install - should skip
    sleep 1
    run dodot install tools
    [ "$status" -eq 0 ]
    
    # Sentinel should not be updated
    local new_time=$(stat -c %Y "$sentinel" 2>/dev/null || stat -f %m "$sentinel")
    [ "$orig_time" = "$new_time" ]
}

@test "homebrew: not processed by deploy command" {
    # Deploy should not process Brewfiles
    run dodot deploy tools
    [ "$status" -eq 0 ]
    
    # Verify Brewfile was NOT processed
    assert_brewfile_not_processed "tools"
}