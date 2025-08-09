#!/usr/bin/env bats
# Test the test framework itself - setup_test_env and clean_test_env functions

# Load test libraries
source /workspace/test-data/lib/setup.sh
source /workspace/test-data/lib/assertions.sh

# We'll need to manage our own setup/teardown since we're testing the framework
setup() {
    # Save current environment for restoration
    export TEST_ORIG_HOME="${HOME:-}"
    export TEST_ORIG_DOTFILES_ROOT="${DOTFILES_ROOT:-}"
    export TEST_ORIG_DODOT_DATA_DIR="${DODOT_DATA_DIR:-}"
    
    # Get the scenario directory
    export TEST_SCENARIO_DIR="$BATS_TEST_DIRNAME/.."
}

teardown() {
    # Always clean up any test environment
    clean_test_env
    
    # Restore original environment
    if [ -n "$TEST_ORIG_HOME" ]; then
        export HOME="$TEST_ORIG_HOME"
    fi
    if [ -n "$TEST_ORIG_DOTFILES_ROOT" ]; then
        export DOTFILES_ROOT="$TEST_ORIG_DOTFILES_ROOT"
    fi
    if [ -n "$TEST_ORIG_DODOT_DATA_DIR" ]; then
        export DODOT_DATA_DIR="$TEST_ORIG_DODOT_DATA_DIR"
    fi
}

@test "setup_test_env: creates temporary directories" {
    # Run setup
    setup_test_env "$TEST_SCENARIO_DIR"
    
    # Verify all test directories were created
    assert_dir_exists "$TEST_HOME"
    assert_dir_exists "$TEST_DOTFILES"
    assert_dir_exists "$TEST_DATA_DIR"
    
    # Verify they are in /tmp with proper names
    [[ "$TEST_HOME" == /tmp/dodot-test-*/test-home-* ]]
    [[ "$TEST_DOTFILES" == /tmp/dodot-test-*/test-dotfiles-* ]]
    [[ "$TEST_DATA_DIR" == /tmp/dodot-test-*/test-dodot-* ]]
}

@test "setup_test_env: copies dotfiles preserving structure" {
    # Run setup
    setup_test_env "$TEST_SCENARIO_DIR"
    
    # Verify dotfiles were copied correctly
    assert_dir_exists "$TEST_DOTFILES/test-pack"
    assert_file_exists "$TEST_DOTFILES/test-pack/testfile.txt"
    
    # Verify content was preserved
    local content=$(cat "$TEST_DOTFILES/test-pack/testfile.txt")
    [ "$content" = "test content" ]
}

@test "setup_test_env: copies home directory correctly" {
    # Run setup
    setup_test_env "$TEST_SCENARIO_DIR"
    
    # Verify home files were copied
    assert_file_exists "$TEST_HOME/.test-home-file"
    
    # Verify content was preserved
    local content=$(cat "$TEST_HOME/.test-home-file")
    [ "$content" = "home test" ]
}

@test "setup_test_env: sets environment variables correctly" {
    # Run setup
    setup_test_env "$TEST_SCENARIO_DIR"
    
    # Verify environment variables are set
    assert_env_set "HOME" "$TEST_HOME"
    assert_env_set "DOTFILES_ROOT" "$TEST_DOTFILES"
    assert_env_set "DODOT_DATA_DIR" "$TEST_DATA_DIR"
    
    # Verify original values are saved (if they were originally set)
    assert_env_set "ORIG_HOME" "$TEST_ORIG_HOME"
    if [ -n "$TEST_ORIG_DOTFILES_ROOT" ]; then
        assert_env_set "ORIG_DOTFILES_ROOT" "$TEST_ORIG_DOTFILES_ROOT"
    fi
    if [ -n "$TEST_ORIG_DODOT_DATA_DIR" ]; then
        assert_env_set "ORIG_DODOT_DATA_DIR" "$TEST_ORIG_DODOT_DATA_DIR"
    fi
}

@test "setup_test_env: handles missing scenario path" {
    # Try with invalid path
    run setup_test_env "/nonexistent/path"
    [ "$status" -ne 0 ]
    [[ "$output" =~ "Invalid scenario path" ]]
}

@test "setup_test_env: handles scenario without dotfiles directory" {
    # Create a temporary scenario without dotfiles
    local temp_scenario="/tmp/test-scenario-$$"
    mkdir -p "$temp_scenario/home"
    echo "test" > "$temp_scenario/home/.testfile"
    
    # Run setup - should warn but not fail
    run setup_test_env "$temp_scenario"
    [ "$status" -eq 0 ]
    [[ "$output" =~ "WARNING: No dotfiles directory" ]]
    
    # Verify empty dotfiles directory was created
    # Need to get the TEST_DOTFILES value from the setup output
    local test_dotfiles=$(echo "$output" | grep "DOTFILES_ROOT=" | cut -d'=' -f2)
    assert_dir_exists "$test_dotfiles"
    
    # Clean up temp scenario
    rm -rf "$temp_scenario"
}

@test "clean_test_env: removes all test directories" {
    # First set up an environment
    setup_test_env "$TEST_SCENARIO_DIR"
    
    # Verify directories exist
    assert_dir_exists "$TEST_HOME"
    assert_dir_exists "$TEST_DOTFILES"
    assert_dir_exists "$TEST_DATA_DIR"
    
    # Save paths for checking after cleanup
    local test_home="$TEST_HOME"
    local test_dotfiles="$TEST_DOTFILES"
    local test_data_dir="$TEST_DATA_DIR"
    
    # Run cleanup
    clean_test_env
    
    # Verify directories were removed
    [ ! -d "$test_home" ]
    [ ! -d "$test_dotfiles" ]
    [ ! -d "$test_data_dir" ]
}

@test "clean_test_env: restores original environment variables" {
    # Set up test environment
    setup_test_env "$TEST_SCENARIO_DIR"
    
    # Run cleanup
    clean_test_env
    
    # Verify original values were restored
    [ "$HOME" = "$TEST_ORIG_HOME" ]
    [ "$DOTFILES_ROOT" = "$TEST_ORIG_DOTFILES_ROOT" ]
    [ "$DODOT_DATA_DIR" = "$TEST_ORIG_DODOT_DATA_DIR" ]
    
    # Verify test variables were unset
    [ -z "$TEST_HOME" ]
    [ -z "$TEST_DOTFILES" ]
    [ -z "$TEST_DATA_DIR" ]
    [ -z "$ORIG_HOME" ]
    [ -z "$ORIG_DOTFILES_ROOT" ]
    [ -z "$ORIG_DODOT_DATA_DIR" ]
}

@test "clean_test_env: safely handles missing directories" {
    # Set test variables without creating directories
    export TEST_HOME="/tmp/nonexistent-home-$$"
    export TEST_DOTFILES="/tmp/nonexistent-dotfiles-$$"
    export TEST_DATA_DIR="/tmp/nonexistent-data-$$"
    
    # Run cleanup - should not fail
    run clean_test_env
    [ "$status" -eq 0 ]
}

@test "clean_test_env: only removes test directories (safety check)" {
    # Create a directory that doesn't match test pattern
    local non_test_dir="/tmp/important-data-$$"
    mkdir -p "$non_test_dir"
    echo "important" > "$non_test_dir/data.txt"
    
    # Set TEST_HOME to non-test directory (simulate misconfiguration)
    export TEST_HOME="$non_test_dir"
    
    # Run cleanup
    clean_test_env
    
    # Verify non-test directory was NOT removed
    assert_dir_exists "$non_test_dir"
    assert_file_exists "$non_test_dir/data.txt"
    
    # Clean up manually
    rm -rf "$non_test_dir"
}

@test "with_test_env: sets up and tears down correctly" {
    # Use with_test_env to run a command
    run with_test_env "$TEST_SCENARIO_DIR" bash -c 'echo "HOME=$HOME"; ls "$TEST_DOTFILES/test-pack/testfile.txt"'
    [ "$status" -eq 0 ]
    [[ "$output" =~ "test-dotfiles-" ]]
    [[ "$output" =~ "testfile.txt" ]]
    
    # Verify cleanup happened (environment should be restored)
    [ "$HOME" = "$TEST_ORIG_HOME" ]
}

@test "setup_test_env: copies dodot-data directory if present" {
    # Create a scenario with dodot-data
    local temp_scenario="/tmp/test-scenario-with-data-$$"
    mkdir -p "$temp_scenario/dotfiles/pack1"
    mkdir -p "$temp_scenario/dodot-data/deployed/symlink"
    echo "test" > "$temp_scenario/dodot-data/deployed/symlink/testfile"
    
    # Run setup
    setup_test_env "$temp_scenario"
    
    # Verify dodot-data was copied
    assert_file_exists "$TEST_DATA_DIR/deployed/symlink/testfile"
    
    # Clean up temp scenario
    rm -rf "$temp_scenario"
}

@test "setup_test_env: sources .envrc files if present" {
    # Create a scenario with .envrc files
    local temp_scenario="/tmp/test-scenario-envrc-$$"
    mkdir -p "$temp_scenario/home"
    mkdir -p "$temp_scenario/dotfiles"
    
    # Create .envrc files that set test variables
    echo 'export TEST_HOME_ENVRC="loaded"' > "$temp_scenario/home/.envrc"
    echo 'export TEST_DOTFILES_ENVRC="loaded"' > "$temp_scenario/dotfiles/.envrc"
    
    # Run setup
    setup_test_env "$temp_scenario"
    
    # Verify .envrc files were sourced
    assert_env_set "TEST_HOME_ENVRC" "loaded"
    assert_env_set "TEST_DOTFILES_ENVRC" "loaded"
    
    # Clean up
    unset TEST_HOME_ENVRC
    unset TEST_DOTFILES_ENVRC
    rm -rf "$temp_scenario"
}