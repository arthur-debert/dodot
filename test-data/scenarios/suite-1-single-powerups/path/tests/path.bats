#!/usr/bin/env bats
# Test path power-up functionality - minimal happy path

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

@test "path: YES - bin directory deployed and accessible" {
    # Deploy tools pack with bin directory  
    dodot_run deploy tools
    [ "$status" -eq 0 ]
    
    # Verify the individual executable was symlinked to home
    [ -L "$HOME/hello" ]
    
    # Verify executable is accessible and works
    [ -x "$HOME/hello" ]
    run "$HOME/hello"
    [ "$status" -eq 0 ]
    [ "$output" = "Hello from tools" ]
}

@test "path: NO - bin directory not deployed (verify absence)" {
    # Create a pack with no bin directory
    mkdir -p "$DOTFILES_ROOT/config"
    echo "config=value" > "$DOTFILES_ROOT/config/settings.conf"
    
    # Verify no executables are symlinked initially
    [ ! -L "$HOME/hello" ]
    [ ! -d "$HOME/bin" ]
    
    # Deploy the config pack (which has no bin directory)
    dodot_run deploy config
    [ "$status" -eq 0 ]
    
    # Verify no executables were symlinked
    [ ! -L "$HOME/hello" ]
    [ ! -d "$HOME/bin" ]
    # Also check that no bin-related symlinks exist in deployed
    [ ! -d "$DODOT_DATA_DIR/deployed/symlink" ] || ! ls "$DODOT_DATA_DIR/deployed/symlink" 2>/dev/null | grep -q "hello"
}