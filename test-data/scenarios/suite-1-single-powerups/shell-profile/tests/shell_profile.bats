#!/usr/bin/env bats
# Minimal test for shell_profile power-up - happy path only

# Load common test setup with debug support
source /workspace/test-data/lib/common.sh

# Setup before all tests
setup() {
    setup_with_debug
}

# Cleanup after each test

# Cleanup after each test
teardown() {
    teardown_with_debug
}

@test "shell_profile: YES - profile sourced in init.sh" {
    # Deploy nvim pack
    dodot_run deploy nvim
    [ "$status" -eq 0 ]
    
    # Verify init.sh was created and contains the profile entry
    assert_profile_in_init "nvim" "profile.sh"
}

@test "shell_profile: NO - profile not sourced (verify absence)" {
    # Create a pack with no profile.sh file (only other files)
    mkdir -p "$DOTFILES_ROOT/vim"
    echo "set number" > "$DOTFILES_ROOT/vim/vimrc"
    
    # Verify init.sh doesn't exist initially
    local init_file="$DODOT_DATA_DIR/shell/init.sh"
    assert_file_not_exists "$init_file"
    
    # Deploy the vim pack (which has no profile.sh)
    dodot_run deploy vim
    [ "$status" -eq 0 ]
    
    # Verify init.sh still doesn't exist (no profiles to source)
    assert_file_not_exists "$init_file"
}