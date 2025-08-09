#!/usr/bin/env bats
# Minimal test for homebrew power-up - happy path only

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh
source /workspace/test-data/lib/assertions_homebrew.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "homebrew: YES - Brewfile processed (sentinel exists)" {
    # Run install command on brew pack
    run dodot install brew
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
    [ ! -f "$DODOT_DATA_DIR/run-once/homebrew/tools" ]
    
    # Install the tools pack (which has no Brewfile)
    run dodot install tools
    [ "$status" -eq 0 ]
    
    # Verify no brew sentinel was created
    [ ! -f "$DODOT_DATA_DIR/run-once/homebrew/tools" ]
}