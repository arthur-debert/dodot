#!/usr/bin/env bats

# Suite 3: Tests multiple power-ups across multiple packs
# This suite validates that dodot correctly handles scenarios where:
# - Multiple packs are present in the dotfiles directory
# - Different packs use different power-ups (symlink, path, install_script, etc.)
# - Power-ups from different packs interact correctly (e.g., multiple PATH entries)
# - Operations are performed in the correct order across packs

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

@test "path: handles multiple bin directories from different packs" {
    # Deploy all three packs with bin directories
    dodot_run deploy tools utils scripts
    [ "$status" -eq 0 ]
    
    # Verify each pack's bin directory was deployed
    assert_path_deployed "tools" "bin"
    assert_path_deployed "utils" "bin"
    assert_path_deployed "scripts" "bin"
    
    # Verify all bin directories are added to init.sh
    assert_path_in_shell_init "$DODOT_DATA_DIR/deployed/path/tools-bin"
    assert_path_in_shell_init "$DODOT_DATA_DIR/deployed/path/utils-bin"
    assert_path_in_shell_init "$DODOT_DATA_DIR/deployed/path/scripts-bin"
    
    # Verify the executables are accessible through symlinks
    assert_symlink_deployed "tools" "bin/tool1" "$HOME/tool1"
    assert_symlink_deployed "utils" "bin/util1" "$HOME/util1"
    assert_symlink_deployed "scripts" "bin/script1" "$HOME/script1"
    
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
    grep -q "tool_version=1.0" "$HOME/tool-config" || fail "File $HOME/tool-config should contain: tool_version=1.0"
    grep -q "util_enabled=true" "$HOME/util-config" || fail "File $HOME/util-config should contain: util_enabled=true"
    grep -q "script_mode=production" "$HOME/script-config" || fail "File $HOME/script-config should contain: script_mode=production"
}

@test "multi-pack deploy: 3 packs each with symlinks" {
    # Deploy git, vim, and shell packs - each has multiple files
    dodot_run deploy git vim shell
    [ "$status" -eq 0 ]
    
    # Verify git pack symlinks
    assert_symlink_deployed "git" "gitconfig" "$HOME/gitconfig"
    assert_symlink_deployed "git" "gitignore" "$HOME/gitignore"
    
    # Verify vim pack symlinks
    assert_symlink_deployed "vim" "vimrc" "$HOME/vimrc"
    assert_symlink_deployed "vim" "gvimrc" "$HOME/gvimrc"
    
    # Verify shell pack symlinks
    assert_symlink_deployed "shell" "bashrc" "$HOME/bashrc"
    assert_symlink_deployed "shell" "zshrc" "$HOME/zshrc"
    
    # Verify content through symlinks
    grep -q "test@example.com" "$HOME/gitconfig" || fail "File $HOME/gitconfig should contain: test@example.com"
    grep -q "set number" "$HOME/vimrc" || fail "File $HOME/vimrc should contain: set number"
    grep -q "PS1=" "$HOME/bashrc" || fail "File $HOME/bashrc should contain: PS1="
}

@test "mixed deploy/install: pack A deploy, pack B install, pack C both" {
    # First install pack B and pack C (install-type handlers)
    dodot_run install install-pack mixed-pack
    [ "$status" -eq 0 ]
    
    # Verify install-pack install script and homebrew
    assert_install_script_executed "install-pack"
    assert_install_artifact_exists "$HOME/.local/install-pack/marker.txt"
    assert_brewfile_processed "install-pack"
    
    # Verify mixed-pack install script and homebrew
    assert_install_script_executed "mixed-pack"
    assert_install_artifact_exists "$HOME/.local/mixed-pack/marker.txt"
    assert_brewfile_processed "mixed-pack"
    
    # Now deploy pack A and pack C (deploy-type handlers)
    dodot_run deploy deploy-pack mixed-pack
    [ "$status" -eq 0 ]
    
    # Verify deploy-pack symlinks
    assert_symlink_deployed "deploy-pack" "deploy-config" "$HOME/deploy-config"
    grep -q "deploy_setting=active" "$HOME/deploy-config" || fail "File $HOME/deploy-config should contain: deploy_setting=active"
    
    # Verify mixed-pack symlinks (it should have both install and deploy working)
    assert_symlink_deployed "mixed-pack" "mixed-config" "$HOME/mixed-config"
    grep -q "mixed_mode=hybrid" "$HOME/mixed-config" || fail "File $HOME/mixed-config should contain: mixed_mode=hybrid"
    
    # Verify that mixed-pack has both install and deploy artifacts
    # Install artifacts from earlier
    assert_file_exists "$HOME/.local/mixed-pack/marker.txt"
    grep -q "mixed-pack-installed" "$HOME/.local/mixed-pack/marker.txt" || fail "File $HOME/.local/mixed-pack/marker.txt should contain: mixed-pack-installed"
    
    # Verify install-pack has no deploy artifacts (it should not be deployed)
    assert_file_not_exists "$HOME/install-pack"
    assert_symlink_not_deployed "$HOME/install-pack"
}