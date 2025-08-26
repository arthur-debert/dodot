#!/usr/bin/env bats

# Suite 2: Multi-Handlers Single Pack
# This suite tests scenarios where multiple handlers are used within single packs.
# It verifies that different handler types can coexist and work together correctly
# when configured in the same pack directory.

# Load common test setup with debug support
source /workspace/live-testing/lib/common.sh

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
    # This should trigger both path (symlink) and shell_add_path handlers
    dodot_run deploy tools
    [ "$status" -eq 0 ]
    
    # Focus on integration: verify both handlers worked together
    # The key integration point is that the deployed path is correctly added to init.sh
    local bin_link="${DODOT_DATA_DIR}/deployed/path/tools-bin"
    assert_path_in_shell_init "$bin_link"
    
    # Verify the integration result: executable is accessible via the PATH
    # This confirms both handlers cooperated successfully
    assert_executable_available "mytool" "tools-bin"
}

# Test: symlink + shell combination in deployment
@test "deploy-type combined: symlink + shell in one pack" {
    # Deploy shell-config pack with both regular files and shell profile
    # This should trigger both symlink and shell handlers
    dodot_run deploy shell-config
    [ "$status" -eq 0 ]
    
    # Focus on integration: verify both handlers deployed from same pack
    # Key test: profile.sh is correctly sourced in init.sh
    assert_profile_in_init "shell-config" "profile.sh"
    
    # Verify key files from the pack are accessible (integration result)
    # Just check one key file to confirm symlinks and profiles coexist
    grep -q "PS1=" "$HOME/bashrc" || fail "bashrc should contain PS1="
}

# Test: install + homebrew combination for installation
@test "install-type combined: install + homebrew in one pack" {
    # Install dev-tools pack with both install.sh and Brewfile
    # This should trigger both install and homebrew handlers
    dodot_run install dev-tools
    [ "$status" -eq 0 ]
    
    # Focus on integration: verify both install-type handlers ran
    # Key test: both install methods completed successfully
    assert_install_executed "dev-tools"
    assert_brewfile_processed "dev-tools"
    
    # Verify integration result: expected artifact from install script
    assert_install_artifact_exists "$HOME/.local/dev-tools/install-marker.txt"
}

# Test: comprehensive pack with all handler types
@test "all handlers: pack with all 5 handler types" {
    # First install to trigger install-type handlers
    dodot_run install ultimate
    [ "$status" -eq 0 ]
    
    # Verify install-type handlers completed
    assert_install_executed "ultimate"
    assert_brewfile_processed "ultimate"
    
    # Now deploy to trigger deploy-type handlers
    dodot_run deploy ultimate
    [ "$status" -eq 0 ]
    
    # Focus on integration: verify all handler types can coexist in one pack
    # Check one key result from each handler type:
    
    # 1. Install script result
    assert_install_artifact_exists "$HOME/.local/ultimate/marker.txt"
    
    # 2. Symlink result
    assert_symlink_deployed "ultimate" "ultimate.conf" "$HOME/ultimate.conf"
    
    # 3. Shell profile integration
    assert_profile_in_init "ultimate" "profile.sh"
    
    # 4. Path deployment result (verify the tool works)
    run "$HOME/ultimate-tool"
    [ "$status" -eq 0 ]
    [ "$output" = "Ultimate tool v1.0" ]
}

# NEGATIVE TESTS: Verify handlers respect command boundaries

# Test: Deploy command should NOT run install-type handlers
@test "negative: deploy on install-only pack (should not run install handlers)" {
    # The dev-tools pack has install.sh and Brewfile (install-only handlers)
    # Running deploy should succeed but NOT execute these handlers
    
    dodot_run deploy dev-tools
    [ "$status" -eq 0 ]
    
    # Verify install-type handlers did NOT run
    assert_install_not_executed "dev-tools"
    assert_brewfile_not_processed "dev-tools"
    
    # The command should succeed (exit 0) but skip inappropriate handlers
    # This ensures deploy respects its boundaries
    echo "PASS: Deploy command correctly skipped install-only handlers"
}

# Test: Install command DOES run deploy-type handlers (but deploy doesn't run install-type)
@test "negative: install vs deploy command boundaries" {
    # Based on the implementation, install runs BOTH install-type and deploy-type handlers
    # This test verifies the asymmetric relationship between commands
    
    # First, verify install runs both types on the ultimate pack
    dodot_run install ultimate
    [ "$status" -eq 0 ]
    
    # Verify BOTH install-type and deploy-type handlers ran
    assert_install_executed "ultimate"
    assert_brewfile_processed "ultimate"
    assert_symlink_deployed "ultimate" "ultimate.conf" "$HOME/ultimate.conf"
    assert_profile_in_init "ultimate" "profile.sh"
    
    # Clean up for the next test
    rm -rf "$DODOT_DATA_DIR"/*
    rm -f "$HOME/ultimate.conf" "$HOME/config" "$HOME/ultimate-tool"
    rm -rf "$HOME/.local/ultimate"
    
    # Now verify deploy ONLY runs deploy-type handlers
    dodot_run deploy ultimate
    [ "$status" -eq 0 ]
    
    # Deploy-type handlers should have run
    assert_symlink_deployed "ultimate" "ultimate.conf" "$HOME/ultimate.conf"
    assert_profile_in_init "ultimate" "profile.sh"
    
    # But install-type handlers should NOT have run
    assert_install_not_executed "ultimate"
    assert_brewfile_not_processed "ultimate"
    
    echo "PASS: Command boundaries verified - install runs all, deploy skips install-type"
}
