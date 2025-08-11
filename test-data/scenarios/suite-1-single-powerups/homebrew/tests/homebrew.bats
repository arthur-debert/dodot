#!/usr/bin/env bats
# Minimal test for homebrew power-up - happy path only

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

@test "homebrew: YES - Brewfile processed (sentinel exists)" {
    # Run install command on brew pack
    dodot_run install brew
    [ "$status" -eq 0 ]
    
    # Verify Brewfile was processed (sentinel created)
    assert_brewfile_processed "brew"
}

@test "homebrew: NO - Brewfile not processed (verify absence)" {
    # Create a pack with no Brewfile
    mkdir -p "$DOTFILES_ROOT/tools"
    echo "#!/bin/bash" > "$DOTFILES_ROOT/tools/install.sh"
    chmod +x "$DOTFILES_ROOT/tools/install.sh"
    
    # Verify no brew sentinel exists initially
    assert_brewfile_not_processed "tools"
    
    # Install the tools pack (which has no Brewfile)
    dodot_run install tools
    [ "$status" -eq 0 ]
    
    # Verify no brew sentinel was created
    assert_brewfile_not_processed "tools"
}