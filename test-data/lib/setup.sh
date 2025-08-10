#!/usr/bin/env bash
# Setup and teardown functions for dodot live system tests

# Safety check for direct sourcing
if [ -z "$DODOT_TEST_CONTAINER" ]; then
    echo "ERROR: This file should not be sourced outside the test container!"
    echo "Tests must be run using: ./containers/dev/run-tests.sh"
    return 1 2>/dev/null || exit 1
fi

# clean_test_env() - Complete cleanup of test environment
# Removes test directories and unsets environment variables
clean_test_env() {
    echo "Cleaning test environment..."
    
    # Save original values if they exist
    local orig_home="${ORIG_HOME:-}"
    local orig_dotfiles="${ORIG_DOTFILES_ROOT:-}"
    local orig_data_dir="${ORIG_DODOT_DATA_DIR:-}"
    
    # Remove test directories if they were created by us
    if [ -n "$TEST_HOME" ] && [ -d "$TEST_HOME" ] && [[ "$TEST_HOME" == */test-home-* ]]; then
        rm -rf "$TEST_HOME"
    fi
    
    if [ -n "$TEST_DOTFILES" ] && [ -d "$TEST_DOTFILES" ] && [[ "$TEST_DOTFILES" == */test-dotfiles-* ]]; then
        rm -rf "$TEST_DOTFILES"
    fi
    
    if [ -n "$TEST_DATA_DIR" ] && [ -d "$TEST_DATA_DIR" ] && [[ "$TEST_DATA_DIR" == */test-dodot-* ]]; then
        rm -rf "$TEST_DATA_DIR"
    fi
    
    # Restore or unset environment variables
    if [ -n "$orig_home" ]; then
        export HOME="$orig_home"
    else
        unset HOME
    fi
    
    if [ -n "$orig_dotfiles" ]; then
        export DOTFILES_ROOT="$orig_dotfiles"
    else
        unset DOTFILES_ROOT
    fi
    
    if [ -n "$orig_data_dir" ]; then
        export DODOT_DATA_DIR="$orig_data_dir"
    else
        unset DODOT_DATA_DIR
    fi
    
    # Clear test variables
    unset TEST_HOME
    unset TEST_DOTFILES
    unset TEST_DATA_DIR
    unset ORIG_HOME
    unset ORIG_DOTFILES_ROOT
    unset ORIG_DODOT_DATA_DIR
    
    # Clear dodot runtime variables
    unset DODOT_SHELL_SOURCE_FLAG
    unset DODOT_DEPLOYMENT_ROOT
}

# setup_test_env() - Set up a fresh test environment
# Args:
#   $1 - scenario path (e.g., test-data/scenarios/basic)
# 
# Creates temporary directories and copies scenario files
setup_test_env() {
    local scenario_path="$1"
    
    if [ -z "$scenario_path" ] || [ ! -d "$scenario_path" ]; then
        echo "ERROR: Invalid scenario path: $scenario_path" >&2
        return 1
    fi
    
    echo "Setting up test environment from: $scenario_path"
    
    # Save original environment
    export ORIG_HOME="${HOME:-}"
    export ORIG_DOTFILES_ROOT="${DOTFILES_ROOT:-}"
    export ORIG_DODOT_DATA_DIR="${DODOT_DATA_DIR:-}"
    
    # Create temporary test directories
    local test_root="/tmp/dodot-test-$$"
    mkdir -p "$test_root"
    
    export TEST_HOME="$test_root/test-home-$$"
    export TEST_DOTFILES="$test_root/test-dotfiles-$$"
    export TEST_DATA_DIR="$test_root/test-dodot-$$"
    
    # Copy scenario directories
    if [ -d "$scenario_path/home" ]; then
        cp -r "$scenario_path/home" "$TEST_HOME"
    else
        mkdir -p "$TEST_HOME"
    fi
    
    if [ -d "$scenario_path/dotfiles" ]; then
        cp -r "$scenario_path/dotfiles"/. "$TEST_DOTFILES/"
    else
        echo "WARNING: No dotfiles directory in scenario" >&2
        mkdir -p "$TEST_DOTFILES"
    fi
    
    # Create dodot data directory (usually empty for fresh tests)
    mkdir -p "$TEST_DATA_DIR"
    
    # If scenario has a dodot-data directory, copy it
    if [ -d "$scenario_path/dodot-data" ]; then
        cp -r "$scenario_path/dodot-data"/* "$TEST_DATA_DIR/"
    fi
    
    # Set test environment variables
    export HOME="$TEST_HOME"
    export DOTFILES_ROOT="$TEST_DOTFILES"
    export DODOT_DATA_DIR="$TEST_DATA_DIR"
    
    # Source any environment setup from scenario
    if [ -f "$TEST_HOME/.envrc" ]; then
        source "$TEST_HOME/.envrc"
    fi
    
    if [ -f "$TEST_DOTFILES/.envrc" ]; then
        source "$TEST_DOTFILES/.envrc"
    fi
    
    echo "Test environment ready:"
    echo "  HOME=$HOME"
    echo "  DOTFILES_ROOT=$DOTFILES_ROOT"
    echo "  DODOT_DATA_DIR=$DODOT_DATA_DIR"
}

# with_test_env() - Run a command in a test environment
# Args:
#   $1 - scenario path
#   $@ - command to run
#
# Sets up environment, runs command, cleans up
with_test_env() {
    local scenario_path="$1"
    shift
    
    # Set up
    if ! setup_test_env "$scenario_path"; then
        return 1
    fi
    
    # Run command
    local exit_code=0
    "$@" || exit_code=$?
    
    # Clean up
    clean_test_env
    
    return $exit_code
}

# ensure_dodot_built() - Ensure dodot binary is built and in PATH
ensure_dodot_built() {
    if ! command -v dodot >/dev/null 2>&1; then
        echo "Building dodot..."
        if [ -f "/workspace/scripts/build" ]; then
            /workspace/scripts/build >/dev/null 2>&1 || {
                echo "ERROR: Failed to build dodot" >&2
                return 1
            }
        else
            echo "ERROR: Build script not found" >&2
            return 1
        fi
    fi
    
    # Add to PATH if needed
    if [[ ":$PATH:" != *":/workspace/bin:"* ]]; then
        export PATH="/workspace/bin:$PATH"
    fi
    
    echo "dodot available: $(which dodot)"
}

# Export functions
export -f clean_test_env
export -f setup_test_env
export -f with_test_env
export -f ensure_dodot_built