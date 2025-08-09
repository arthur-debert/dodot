#!/usr/bin/env bats
# Test path power-up functionality

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

@test "path: deploys bin directory" {
    # Deploy tools pack with bin directory
    run dodot deploy tools
    [ "$status" -eq 0 ]
    
    # Verify bin directory is deployed
    assert_path_deployed "tools" "bin"
    
    # Verify executable is accessible through deployed path
    assert_executable_available "hello-dodot" "tools-bin"
}

@test "path: adds directory to PATH in init.sh" {
    # Deploy tools pack
    run dodot deploy tools
    [ "$status" -eq 0 ]
    
    # Check both path and shell_add_path power-ups ran
    # Path power-up creates the symlink
    assert_path_deployed "tools" "bin"
    
    # Shell_add_path power-up adds to init.sh
    local deployed_path="$DODOT_DATA_DIR/deployed/path/tools-bin"
    assert_path_in_shell_init "$deployed_path"
}

@test "path: handles multiple bin directories" {
    # Create another pack with bin
    mkdir -p "$DOTFILES_ROOT/utils/bin"
    echo '#!/bin/bash' > "$DOTFILES_ROOT/utils/bin/test-util"
    chmod +x "$DOTFILES_ROOT/utils/bin/test-util"
    
    # Deploy both packs
    run dodot deploy tools utils
    [ "$status" -eq 0 ]
    
    # Both should be deployed
    assert_path_deployed "tools" "bin"
    assert_path_deployed "utils" "bin"
}