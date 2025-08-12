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
    
    # Verify bin directory is deployed to path
    assert_path_deployed "tools" "bin"
    
    # Verify PATH addition is in shell init
    assert_path_in_shell_init "$DODOT_DATA_DIR/deployed/path/tools-bin"
    
    # Verify executable is available through deployed path
    assert_executable_available "hello" "tools-bin"
    
    # Source the init.sh to update PATH
    source "$DODOT_DATA_DIR/shell/init.sh"
    
    # CRITICAL: Verify scripts in bin were NOT sourced
    # The hello script sets HELLO_WAS_SOURCED=1 if sourced
    [ -z "$HELLO_WAS_SOURCED" ] || fail "Script was sourced when it should only be added to PATH"
    
    # Verify the directory is actually in the PATH environment variable
    assert_path_in_env "$DODOT_DATA_DIR/deployed/path/tools-bin"
    
    # Verify the executable can be found via which
    run which hello
    [ "$status" -eq 0 ]
    [[ "$output" == *"tools-bin/hello"* ]]
    
    # Run the executable to verify it works
    run hello
    [ "$status" -eq 0 ]
    [ "$output" = "Hello from tools" ]
    
    # Double-check: Even after running, the script should not have been sourced
    [ -z "$HELLO_WAS_SOURCED" ] || fail "Script was sourced during execution"
}

@test "path: NO - bin directory not deployed (verify absence)" {
    # Create a pack with no bin directory
    mkdir -p "$DOTFILES_ROOT/config"
    echo "config=value" > "$DOTFILES_ROOT/config/settings.conf"
    
    # Deploy the config pack (which has no bin directory)
    dodot_run deploy config
    [ "$status" -eq 0 ]
    
    # Verify no path deployment occurred
    [ ! -d "$DODOT_DATA_DIR/deployed/path" ] || [ -z "$(ls -A "$DODOT_DATA_DIR/deployed/path" 2>/dev/null)" ]
    
    # Verify init.sh doesn't contain path exports for this pack
    local init_file="${DODOT_DATA_DIR}/shell/init.sh"
    if [ -f "$init_file" ]; then
        ! grep -q "deployed/path/config" "$init_file"
    fi
}