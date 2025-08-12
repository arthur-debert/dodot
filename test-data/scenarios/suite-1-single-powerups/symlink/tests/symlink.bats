#!/usr/bin/env bats
# Minimal test for symlink power-up - happy path only

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

@test "symlink: YES - deployed successfully" {
    # Deploy git pack
    dodot_run deploy git
    [ "$status" -eq 0 ]
    
    # Verify symlink was created correctly
    assert_symlink_deployed "git" "gitconfig" "$HOME/gitconfig"
}

@test "symlink: NO - not deployed (verify absence)" {
    # Test Case 1: Empty pack should have no symlinks
    mkdir -p "$DOTFILES_ROOT/empty-pack"
    
    # Deploy the empty pack
    dodot_run deploy empty-pack
    [ "$status" -eq 0 ]
    
    # Verify NO symlinks were created for the empty pack
    assert_no_symlinks_for_pack "empty-pack"
    
    # Test Case 2: Pack with only higher-priority power-up files
    mkdir -p "$DOTFILES_ROOT/priority-pack"
    
    # Add files that are handled by other power-ups with higher priority than symlink
    # Install script - handled by install_script power-up
    cat > "$DOTFILES_ROOT/priority-pack/install.sh" << 'EOF'
#!/bin/bash
echo "Installing"
EOF
    chmod +x "$DOTFILES_ROOT/priority-pack/install.sh"
    
    # Brewfile - handled by homebrew power-up  
    cat > "$DOTFILES_ROOT/priority-pack/Brewfile" << 'EOF'
brew "tree"
EOF
    
    # Shell profile - handled by shell_profile power-up
    cat > "$DOTFILES_ROOT/priority-pack/profile.sh" << 'EOF'
export VAR=value
EOF
    
    # Deploy the priority pack
    dodot_run deploy priority-pack
    [ "$status" -eq 0 ]
    
    # Verify NO symlinks were created - all files handled by other power-ups
    assert_no_symlinks_for_pack "priority-pack"
    
    # Verify these specific files weren't symlinked to HOME
    assert_symlink_not_deployed "$HOME/install.sh"
    assert_symlink_not_deployed "$HOME/Brewfile" 
    assert_symlink_not_deployed "$HOME/profile.sh"
    
    # Test Case 3: Verify our new assertion catches actual problems
    # Create a pack that WILL have symlinks to contrast with above
    mkdir -p "$DOTFILES_ROOT/config-pack"
    echo "config=value" > "$DOTFILES_ROOT/config-pack/app.conf"
    
    dodot_run deploy config-pack
    [ "$status" -eq 0 ]
    
    # This pack SHOULD have symlinks - verify our assertion would catch it
    if assert_no_symlinks_for_pack "config-pack" 2>/dev/null; then
        fail "assert_no_symlinks_for_pack should have failed for config-pack but didn't"
    fi
    
    # Verify the symlink was actually created (positive control)
    assert_symlink_deployed "config-pack" "app.conf" "$HOME/app.conf"
    
    echo "PASS: NO case properly tested - empty and priority-only packs have no symlinks"
}