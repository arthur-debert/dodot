#!/usr/bin/env bats

# Suite 3: Tests multiple power-ups across multiple packs
# This suite validates that dodot correctly handles scenarios where:
# - Multiple packs are present in the dotfiles directory
# - Different packs use different power-ups (symlink, path, install_script, etc.)
# - Power-ups from different packs interact correctly (e.g., multiple PATH entries)
# - Operations are performed in the correct order across packs

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

@test "path: handles multiple bin directories from different packs" {
    # Deploy all three packs with bin directories
    dodot_run deploy tools utils scripts
    [ "$status" -eq 0 ]
    
    # Verify each pack's bin directory was deployed
    assert_path_deployed "tools" "bin"
    assert_path_deployed "utils" "bin"
    assert_path_deployed "scripts" "bin"
    
    # Verify all bin directories are added to init.sh
    local init_file="${DODOT_DATA_DIR}/shell/init.sh"
    [ -f "$init_file" ]
    
    # Check that all three paths are in init.sh
    grep -q "deployed/path/tools-bin" "$init_file"
    grep -q "deployed/path/utils-bin" "$init_file"
    grep -q "deployed/path/scripts-bin" "$init_file"
    
    # Verify the executables are accessible through symlinks
    [ -L "$HOME/tool1" ]
    [ -L "$HOME/util1" ]
    [ -L "$HOME/script1" ]
    
    # Verify running each tool works
    run "$HOME/tool1"
    [ "$status" -eq 0 ]
    [ "$output" = "tool1 from tools pack" ]
    
    run "$HOME/util1"
    [ "$status" -eq 0 ]
    [ "$output" = "util1 from utils pack" ]
    
    run "$HOME/script1"
    [ "$status" -eq 0 ]
    [ "$output" = "script1 from scripts pack" ]
}

@test "symlink: deploy multiple files from different packs" {
    # Deploy all three packs with config files
    dodot_run deploy tools utils scripts
    [ "$status" -eq 0 ]
    
    # Verify symlinks from each pack
    assert_symlink_deployed "tools" "tool-config" "$HOME/tool-config"
    assert_symlink_deployed "utils" "util-config" "$HOME/util-config"
    assert_symlink_deployed "scripts" "script-config" "$HOME/script-config"
    
    # Verify content is accessible through symlinks
    grep -q "tool_version=1.0" "$HOME/tool-config"
    grep -q "util_enabled=true" "$HOME/util-config"
    grep -q "script_mode=production" "$HOME/script-config"
    
    # Verify all files are symlinks pointing to the right sources
    [ -L "$HOME/tool-config" ]
    [ -L "$HOME/util-config" ]
    [ -L "$HOME/script-config" ]
    
    # Verify symlinks point to correct source files
    local tool_target=$(readlink "$HOME/tool-config")
    local util_target=$(readlink "$HOME/util-config")
    local script_target=$(readlink "$HOME/script-config")
    
    [[ "$tool_target" == *"deployed/symlink/tool-config"* ]]
    [[ "$util_target" == *"deployed/symlink/util-config"* ]]
    [[ "$script_target" == *"deployed/symlink/script-config"* ]]
}

@test "multi-pack deploy: 3 packs each with symlinks" {
    skip "Not implemented"
}

@test "mixed deploy/install: pack A deploy, pack B install, pack C both" {
    skip "Not implemented"
}