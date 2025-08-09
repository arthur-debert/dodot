#!/usr/bin/env bats
# Test install_script power-up functionality

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh
source /workspace/test-data/lib/assertions_install.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "install_script: executes install.sh on first install" {
    # Run install command
    run dodot install tools
    [ "$status" -eq 0 ]
    
    # Verify install script was executed
    assert_install_script_executed "tools"
    
    # Verify the script created its marker file
    assert_install_artifact_exists "$HOME/.local/dodot-test/tools-installed.txt"
}

@test "install_script: skips execution on second install without force" {
    # First install
    run dodot install tools
    [ "$status" -eq 0 ]
    assert_install_script_executed "tools"
    
    # Remove marker file
    rm -f "$HOME/.local/dodot-test/tools-installed.txt"
    
    # Second install - should skip
    run dodot install tools
    [ "$status" -eq 0 ]
    
    # Marker file should NOT be recreated
    assert_file_exists "$DODOT_DATA_DIR/install/sentinels/tools"
    [ ! -f "$HOME/.local/dodot-test/tools-installed.txt" ]
}

@test "install_script: not executed by deploy command" {
    # Deploy should not run install scripts
    run dodot deploy tools
    [ "$status" -eq 0 ]
    
    # Verify install script was NOT executed
    assert_install_script_not_executed "tools"
}