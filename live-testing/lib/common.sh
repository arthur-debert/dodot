#!/usr/bin/env bash
# Common test setup that includes all necessary libraries and debug hooks
# All test files should source this single file instead of individual libraries

# Safety check: Ensure we're running in a container (unless testing the test framework itself)
if [ -z "$DODOT_TEST_CONTAINER" ] || [ "$DODOT_TEST_CONTAINER" != "1" ]; then
    # Allow test-framework self-tests to bypass this check
    if [ "$DODOT_TEST_FRAMEWORK_SELF_TEST" != "1" ]; then
        echo "ERROR: Tests must be run inside the Docker container!"
        echo "Use: ./scripts/run-live-tests"
        exit 1
    fi
fi

# Source all test libraries
source /workspace/live-testing/lib/setup.sh
source /workspace/live-testing/lib/assertions.sh
source /workspace/live-testing/lib/debug.sh

# Load power-up specific assertions
for assertion_file in /workspace/live-testing/lib/assertions_*.sh; do
    [ -f "$assertion_file" ] && source "$assertion_file"
done

# Enhanced setup function with debug hooks
setup_with_debug() {
    # Set test start time for performance tracking
    export BATS_TEST_START_TIME=$(date +%s)
    
    # Clear previous test state
    unset LAST_DODOT_COMMAND
    unset LAST_DODOT_EXIT_CODE
    unset LAST_DODOT_OUTPUT
    
    # Ensure dodot is built
    ensure_dodot_built
    
    # Setup test environment
    setup_test_env "$BATS_TEST_DIRNAME/.."
    
    # Set up trap for debug on failure
    trap 'debug_on_fail' ERR
}

# Enhanced teardown function with debug hooks
teardown_with_debug() {
    # Remove error trap
    trap - ERR
    
    # If test failed, show debug info
    if [ "${BATS_TEST_COMPLETED:-1}" -ne 0 ]; then
        debug_on_fail
    fi
    
    # Clean test environment
    clean_test_env
}

# Wrapper for dodot commands to capture debug info
dodot_run() {
    # Capture timing
    export DODOT_COMMAND_START_TIME=$(date +%s)
    
    # Build full command
    local cmd="dodot $*"
    export LAST_DODOT_COMMAND="$cmd"
    
    # Use Bats run command
    run dodot "$@"
    
    # Capture results
    export LAST_DODOT_EXIT_CODE="$status"
    export LAST_DODOT_OUTPUT="$output"
}

# Export the enhanced functions
export -f setup_with_debug
export -f teardown_with_debug
export -f dodot_run