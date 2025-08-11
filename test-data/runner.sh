#!/usr/bin/env bash
# Minimal test runner - just safety checks and pass everything to Bats

set -e

# Safety check: ONLY run tests inside Docker container
if [ ! -f "/.dockerenv" ] && [ ! -f "/run/.containerenv" ]; then
    echo "ERROR: TESTS MUST RUN INSIDE DOCKER CONTAINER"
    echo "Running tests outside container could damage your system!"
    echo ""
    echo "Please use: ./containers/dev/run-tests.sh"
    exit 1
fi

# Set test environment marker
export DODOT_TEST_CONTAINER=1

# Ensure dodot is built
if [ ! -x "/workspace/bin/dodot" ]; then
    echo "Building dodot..."
    /workspace/scripts/build || {
        echo "ERROR: Failed to build dodot"
        exit 1
    }
fi
export PATH="/workspace/bin:$PATH"

# If no args provided, run all tests
if [ $# -eq 0 ]; then
    # Find all test files
    test_files=()
    while IFS= read -r -d '' file; do
        test_files+=("$file")
    done < <(find /workspace/test-data/scenarios -name "*.bats" -type f -print0 | sort -z)
    
    if [ ${#test_files[@]} -eq 0 ]; then
        echo "ERROR: No test files found"
        exit 1
    fi
    
    set -- "${test_files[@]}"
fi

# Just run bats with whatever args we have
exec bats "$@"