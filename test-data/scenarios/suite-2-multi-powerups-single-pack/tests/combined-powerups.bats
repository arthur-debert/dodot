#!/usr/bin/env bats

# Suite 2: Multi-PowerUps Single Pack
# This suite tests scenarios where multiple power-ups are used within single packs.
# It verifies that different power-up types can coexist and work together correctly
# when configured in the same pack directory.

# Load common test setup with debug support
source /workspace/test-data/lib/common.sh

# Setup before all tests
setup() {
    setup_with_debug
}

# Cleanup after each test
teardown() {
    teardown_with_debug
}

# Test: path + shell_add_path combination
@test "path + shell_add_path: adds directory to PATH in init.sh" {
    # Deploy tools pack with bin directory
    # This should trigger both path (symlink) and shell_add_path powerups
    dodot_run deploy tools
    [ "$status" -eq 0 ]
    
    # Focus on integration: verify both powerups worked together
    # The key integration point is that the deployed path is correctly added to init.sh
    local bin_link="${DODOT_DATA_DIR}/deployed/path/tools-bin"
    assert_path_in_shell_init "$bin_link"
    
    # Verify the integration result: executable is accessible via the PATH
    # This confirms both powerups cooperated successfully
    assert_executable_available "mytool" "tools-bin"
}

# Test: symlink + shell_profile combination in deployment
@test "deploy-type combined: symlink + shell_profile in one pack" {
    # Deploy shell-config pack with both regular files and shell profile
    # This should trigger both symlink and shell_profile powerups
    dodot_run deploy shell-config
    [ "$status" -eq 0 ]
    
    # Focus on integration: verify both powerups deployed from same pack
    # Key test: profile.sh is correctly sourced in init.sh
    assert_profile_in_init "shell-config" "profile.sh"
    
    # Verify key files from the pack are accessible (integration result)
    # Just check one key file to confirm symlinks and profiles coexist
    assert_template_contains "$HOME/bashrc" "PS1="
}

# Test: install_script + homebrew combination for installation
@test "install-type combined: install_script + homebrew in one pack" {
    # Install dev-tools pack with both install.sh and Brewfile
    # This should trigger both install_script and homebrew powerups
    dodot_run install dev-tools
    [ "$status" -eq 0 ]
    
    # Focus on integration: verify both install-type powerups ran
    # Key test: both install methods completed successfully
    assert_install_script_executed "dev-tools"
    assert_brewfile_processed "dev-tools"
    
    # Verify integration result: expected artifact from install script
    assert_install_artifact_exists "$HOME/.local/dev-tools/install-marker.txt"
}

# Test: comprehensive pack with all power-up types
@test "all powerups: pack with all 6 power-up types" {
    # First install to trigger install-type powerups
    dodot_run install ultimate
    [ "$status" -eq 0 ]
    
    # Verify install-type powerups completed
    assert_install_script_executed "ultimate"
    assert_brewfile_processed "ultimate"
    
    # Now deploy to trigger deploy-type powerups
    dodot_run deploy ultimate
    [ "$status" -eq 0 ]
    
    # Focus on integration: verify all powerup types can coexist in one pack
    # Check one key result from each powerup type:
    
    # 1. Install script result
    assert_install_artifact_exists "$HOME/.local/ultimate/marker.txt"
    
    # 2. Symlink result
    assert_symlink_deployed "ultimate" "ultimate.conf" "$HOME/ultimate.conf"
    
    # 3. Template processing result
    assert_template_contains "$HOME/config" "username = $USER"
    
    # 4. Shell profile integration
    assert_profile_in_init "ultimate" "profile.sh"
    
    # 5. Path deployment result (verify the tool works)
    run "$HOME/ultimate-tool"
    [ "$status" -eq 0 ]
    [ "$output" = "Ultimate tool v1.0" ]
}
