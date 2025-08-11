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
    # Create a pack with only install-type files (no symlink candidates)
    mkdir -p "$DOTFILES_ROOT/tools"
    cat > "$DOTFILES_ROOT/tools/install.sh" << 'EOF'
#!/bin/bash
echo "Installing tools"
EOF
    chmod +x "$DOTFILES_ROOT/tools/install.sh"
    
    # Verify no symlinks exist initially
    assert_symlink_not_deployed "$HOME/gitconfig"
    
    # Deploy the tools pack (which has no files for symlink power-up)
    dodot_run deploy tools
    [ "$status" -eq 0 ]
    
    # Verify no symlinks were created
    assert_symlink_not_deployed "$HOME/gitconfig"
    # Also verify the deployed symlink directory is empty or doesn't exist
    local symlink_dir="$DODOT_DATA_DIR/deployed/symlink"
    if [ -d "$symlink_dir" ]; then
        [ -z "$(ls -A "$symlink_dir" 2>/dev/null)" ] || {
            echo "FAIL: Symlink directory is not empty: $symlink_dir" >&2
            ls -la "$symlink_dir" >&2
            return 1
        }
    fi
}