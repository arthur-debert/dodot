#!/usr/bin/env bats
# Test path power-up functionality - minimal happy path

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh
source /workspace/test-data/lib/assertions_path.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "path: deploys bin directory and adds to PATH in init.sh" {
    # Deploy tools pack with bin directory
    run dodot deploy tools
    [ "$status" -eq 0 ]
    
    # Verify bin directory is deployed
    assert_path_deployed "tools" "bin"
    
    # Verify executable is accessible through deployed path
    assert_executable_available "hello" "tools-bin"
    
    # Verify path is added to init.sh by shell_add_path power-up
    local deployed_path="$DODOT_DATA_DIR/deployed/path/tools-bin"
    assert_path_in_shell_init "$deployed_path"
}