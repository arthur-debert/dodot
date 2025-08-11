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
    skip "Migrated from basic scenario - not implemented"
}

@test "multi-pack deploy: 3 packs each with symlinks" {
    skip "Not implemented"
}

@test "mixed deploy/install: pack A deploy, pack B install, pack C both" {
    skip "Not implemented"
}