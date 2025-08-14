#!/usr/bin/env bats

# Test for cross-pack symlink conflict detection (issue #548)
# This test verifies that dodot properly detects when multiple packs
# try to create symlinks with the same target path

load test_helper

@test "symlink conflict: two packs with same filename" {
    # Create first pack with config.toml
    create_pack "tool-1"
    create_file "tool-1/config.toml" "[tool1]\nname = \"Tool 1\""
    
    # Create second pack with same filename
    create_pack "tool-2"
    create_file "tool-2/config.toml" "[tool2]\nname = \"Tool 2\""
    
    # Deploy should fail with conflict error
    run_dodot deploy
    assert_failure
    assert_output_contains "symlink conflict detected"
    assert_output_contains "tool-1/config.toml"
    assert_output_contains "tool-2/config.toml"
    
    # Verify no symlinks were created
    assert_not_exists "$HOME/config.toml"
}

@test "symlink conflict: three packs with same filename" {
    # Create three packs with the same filename
    create_pack "app-1"
    create_file "app-1/.bashrc" "# App 1 bashrc"
    
    create_pack "app-2"
    create_file "app-2/.bashrc" "# App 2 bashrc"
    
    create_pack "app-3"
    create_file "app-3/.bashrc" "# App 3 bashrc"
    
    # Deploy should fail with conflict error
    run_dodot deploy
    assert_failure
    assert_output_contains "symlink conflict detected"
    assert_output_contains "app-1/.bashrc"
    assert_output_contains "app-2/.bashrc"
    assert_output_contains "app-3/.bashrc"
}

@test "symlink no conflict: different filenames" {
    # Create packs with different filenames - should work
    create_pack "vim"
    create_file "vim/.vimrc" "\" Vim config"
    
    create_pack "bash"
    create_file "bash/.bashrc" "# Bash config"
    
    # Deploy should succeed
    run_dodot deploy
    assert_success
    
    # Both symlinks should be created
    assert_exists "$HOME/.vimrc"
    assert_exists "$HOME/.bashrc"
    assert_symlink "$HOME/.vimrc" "$DOTFILES_ROOT/vim/.vimrc"
    assert_symlink "$HOME/.bashrc" "$DOTFILES_ROOT/bash/.bashrc"
}

@test "symlink no conflict: same basename in different paths" {
    # Create packs with same basename but different paths
    create_pack "app-1"
    create_file "app-1/.config/app1/settings.json" '{"app": "1"}'
    
    create_pack "app-2"
    create_file "app-2/.config/app2/settings.json" '{"app": "2"}'
    
    # Deploy should succeed - different target paths
    run_dodot deploy
    assert_success
    
    # Both symlinks should be created
    assert_exists "$HOME/.config/app1/settings.json"
    assert_exists "$HOME/.config/app2/settings.json"
}

@test "symlink conflict: detected in dry-run mode" {
    # Create conflicting packs
    create_pack "tool-a"
    create_file "tool-a/shared.conf" "Tool A config"
    
    create_pack "tool-b"
    create_file "tool-b/shared.conf" "Tool B config"
    
    # Dry run should also detect the conflict
    run_dodot deploy --dry-run
    assert_failure
    assert_output_contains "symlink conflict detected"
    assert_output_contains "tool-a/shared.conf"
    assert_output_contains "tool-b/shared.conf"
    
    # Verify no symlinks were created
    assert_not_exists "$HOME/shared.conf"
}

@test "symlink conflict: single pack deployment succeeds" {
    # Create conflicting packs
    create_pack "tool-x"
    create_file "tool-x/config.yaml" "tool: x"
    
    create_pack "tool-y"
    create_file "tool-y/config.yaml" "tool: y"
    
    # Deploying single pack should succeed
    run_dodot deploy tool-x
    assert_success
    assert_exists "$HOME/config.yaml"
    assert_symlink "$HOME/config.yaml" "$DOTFILES_ROOT/tool-x/config.yaml"
    
    # But deploying the second pack should fail
    run_dodot deploy tool-y
    assert_failure
    # Note: This might not detect conflict since tool-x is already deployed
    # This behavior is expected as we only check conflicts within the deployment set
}

@test "symlink conflict: resolution by renaming" {
    # Create conflicting packs
    create_pack "first"
    create_file "first/config" "First config"
    
    create_pack "second"
    create_file "second/config" "Second config"
    
    # Initial deploy should fail
    run_dodot deploy
    assert_failure
    assert_output_contains "symlink conflict"
    
    # Rename one of the files
    mv "$DOTFILES_ROOT/second/config" "$DOTFILES_ROOT/second/config.second"
    
    # Now deploy should succeed
    run_dodot deploy
    assert_success
    assert_exists "$HOME/config"
    assert_exists "$HOME/config.second"
}