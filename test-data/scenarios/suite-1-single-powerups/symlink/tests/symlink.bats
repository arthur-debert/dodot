#!/usr/bin/env bats
# Minimal test for symlink power-up - happy path only

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "symlink: YES - deployed successfully" {
    # Deploy git pack
    run dodot deploy git
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
    run dodot deploy tools
    [ "$status" -eq 0 ]
    
    # Verify no symlinks were created
    assert_symlink_not_deployed "$HOME/gitconfig"
    [ ! -d "$DODOT_DATA_DIR/deployed/symlink" ] || [ -z "$(ls -A "$DODOT_DATA_DIR/deployed/symlink" 2>/dev/null)" ]
}