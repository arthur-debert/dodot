#!/usr/bin/env bats
# Test path power-up functionality - minimal happy path

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh
source /workspace/test-data/lib/assertions_path.sh

# Setup before all tests
setup() {
    ensure_dodot_built
    setup_test_env "$BATS_TEST_DIRNAME/.."
}

# Cleanup after each test
teardown() {
    clean_test_env
}

@test "path: YES - bin directory deployed and accessible" {
    # Deploy tools pack with bin directory  
    run dodot deploy tools
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
    run dodot deploy config
    [ "$status" -eq 0 ]
    
    # Verify no executables were symlinked
    [ ! -L "$HOME/hello" ]
    [ ! -d "$HOME/bin" ]
    # Also check that no bin-related symlinks exist in deployed
    [ ! -d "$DODOT_DATA_DIR/deployed/symlink" ] || ! ls "$DODOT_DATA_DIR/deployed/symlink" 2>/dev/null | grep -q "hello"
}