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
    
    # Verify all 6 symlinks exist
    local expected_files=("gitconfig" "gitignore" "vimrc" "gvimrc" "bashrc" "zshrc")
    for file in "${expected_files[@]}"; do
        [ -L "$HOME/$file" ] || {
            echo "ERROR: Expected symlink $HOME/$file not found"
            return 1
        }
    done
    
    # Verify content through symlinks
    grep -q "test@example.com" "$HOME/gitconfig"
    grep -q "set number" "$HOME/vimrc"
    grep -q "PS1=" "$HOME/bashrc"
    
    # Verify pack isolation - each file points to its own pack
    local gitconfig_target=$(readlink "$HOME/gitconfig")
    local vimrc_target=$(readlink "$HOME/vimrc")
    local bashrc_target=$(readlink "$HOME/bashrc")
    
    [[ "$gitconfig_target" == *"deployed/symlink/gitconfig"* ]]
    [[ "$vimrc_target" == *"deployed/symlink/vimrc"* ]]
    [[ "$bashrc_target" == *"deployed/symlink/bashrc"* ]]
}

@test "mixed deploy/install: pack A deploy, pack B install, pack C both" {
    # First install pack B and pack C (install-type powerups)
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
    
    # Now deploy pack A and pack C (deploy-type powerups)
    dodot_run deploy deploy-pack mixed-pack
    [ "$status" -eq 0 ]
    
    # Verify deploy-pack symlinks
    assert_symlink_deployed "deploy-pack" "deploy-config" "$HOME/deploy-config"
    grep -q "deploy_setting=active" "$HOME/deploy-config"
    
    # Verify mixed-pack symlinks (it should have both install and deploy working)
    assert_symlink_deployed "mixed-pack" "mixed-config" "$HOME/mixed-config"
    grep -q "mixed_mode=hybrid" "$HOME/mixed-config"
    
    # Verify that mixed-pack has both install and deploy artifacts
    # Install artifacts from earlier
    [ -f "$HOME/.local/mixed-pack/marker.txt" ]
    grep -q "mixed-pack-installed" "$HOME/.local/mixed-pack/marker.txt"
    
    # Deploy artifacts now
    [ -L "$HOME/mixed-config" ]
    
    # Verify install-pack has no deploy artifacts (it should not be deployed)
    [ ! -f "$HOME/install-pack" ]
    [ ! -L "$HOME/install-pack" ]
}