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
    
    # Verify path powerup: bin directory is deployed
    assert_path_deployed "tools" "bin"
    
    # Verify the bin directory symlink exists in dodot data
    local bin_link="${DODOT_DATA_DIR}/deployed/path/tools-bin"
    [ -L "$bin_link" ]
    
    # Verify shell_add_path powerup: check init.sh contains PATH addition
    local init_file="${DODOT_DATA_DIR}/shell/init.sh"
    [ -f "$init_file" ]
    grep -q "export PATH=\"${bin_link}:\$PATH\"" "$init_file"
    
    # Note: The actual tool files are symlinked individually, not the directory
    # So we check for the individual symlink
    [ -L "$HOME/mytool" ]
    
    # Verify running the tool works through the symlink
    run "$HOME/mytool"
    [ "$status" -eq 0 ]
    [ "$output" = "mytool from tools pack" ]
}

# Test: symlink + shell_profile combination in deployment
@test "deploy-type combined: symlink + shell_profile in one pack" {
    # Deploy shell-config pack with both regular files and shell profile
    # This should trigger both symlink and shell_profile powerups
    dodot_run deploy shell-config
    [ "$status" -eq 0 ]
    
    # Verify shell_profile powerup: profile.sh is added to init.sh
    assert_profile_in_init "shell-config" "profile.sh"
    
    # Verify symlink powerup: regular files are symlinked
    # Note: dodot creates symlinks without dot prefix in HOME
    assert_symlink_deployed "shell-config" "bashrc" "$HOME/bashrc"
    assert_symlink_deployed "shell-config" "gitconfig" "$HOME/gitconfig"
    
    # Verify content is accessible through symlinks
    grep -q "PS1=" "$HOME/bashrc"
    grep -q "test@example.com" "$HOME/gitconfig"
}

# Test: install_script + homebrew combination for installation
@test "install-type combined: install_script + homebrew in one pack" {
    # Install dev-tools pack with both install.sh and Brewfile
    # This should trigger both install_script and homebrew powerups
    dodot_run install dev-tools
    [ "$status" -eq 0 ]
    
    # Verify install_script powerup: script was executed
    assert_install_script_executed "dev-tools"
    
    # Verify install script created its marker
    assert_install_artifact_exists "$HOME/.local/dev-tools/install-marker.txt"
    
    # Verify marker content
    run cat "$HOME/.local/dev-tools/install-marker.txt"
    [ "$status" -eq 0 ]
    [ "$output" = "dev-tools-installed" ]
    
    # Verify homebrew powerup: Brewfile was processed
    assert_brewfile_processed "dev-tools"
}

# Test: comprehensive pack with all power-up types
@test "all powerups: pack with all 6 power-up types" {
    skip "Not implemented"
}
