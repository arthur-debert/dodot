#!/usr/bin/env bats
# Minimal test for shell_profile power-up - happy path only

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh
source /workspace/test-data/lib/assertions_shell.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "shell_profile: YES - profile sourced in init.sh" {
    # Deploy nvim pack
    run dodot deploy nvim
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
    [ ! -f "$init_file" ]
    
    # Deploy the vim pack (which has no profile.sh)
    run dodot deploy vim
    [ "$status" -eq 0 ]
    
    # Verify init.sh still doesn't exist (no profiles to source)
    [ ! -f "$init_file" ]
}