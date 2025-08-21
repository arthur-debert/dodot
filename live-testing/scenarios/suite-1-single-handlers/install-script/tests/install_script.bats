#!/usr/bin/env bats
# Minimal test for install_script handler - happy path only

# Load common test setup with debug support
source /workspace/live-testing/lib/common.sh

# Setup before all tests
setup() {
    setup_with_debug
}

# Cleanup after each test

# Cleanup after each test
teardown() {
    teardown_with_debug
}

@test "install_script: YES - script executed (marker created)" {
    # Install dev pack
    dodot_run install dev
    [ "$status" -eq 0 ]
    
    # Use proper assertion helpers
    assert_install_script_executed "dev"
    
    # Verify the marker file was created using the helper
    assert_install_artifact_exists "$HOME/.local/test/marker.txt"
    
    # Verify the marker file contains the expected content
    run cat "$HOME/.local/test/marker.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "installed" ]
}

@test "install_script: NO - script not executed (verify absence)" {
    # Create a pack with no install.sh file
    mkdir -p "$DOTFILES_ROOT/config"
    echo "config=value" > "$DOTFILES_ROOT/config/settings.conf"
    
    # Deploy the config pack (which has no install.sh)
    dodot_run deploy config
    [ "$status" -eq 0 ]
    
    # Use proper assertion helper
    assert_install_script_not_executed "config"
}