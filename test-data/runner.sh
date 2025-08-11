#!/usr/bin/env bash
# Simplified test runner using Bats native formatters
# This runner leverages Bats' built-in capabilities instead of custom logic

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
echo "Ensuring dodot is built..."
if [ -x "/workspace/bin/dodot" ]; then
    echo "dodot already built"
else
    echo "Building dodot..."
    /workspace/scripts/build || {
        echo "ERROR: Failed to build dodot"
        exit 1
    }
fi
export PATH="/workspace/bin:$PATH"

# Set default test pattern if no args provided
if [ $# -eq 0 ]; then
    # Find all test files - bats doesn't expand ** globs
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

# Determine output format
JUNIT_FILE="/workspace/test-results.xml"
HUMAN_OUTPUT=true

# Check if we should generate JUnit output
for arg in "$@"; do
    if [[ "$arg" == "--formatter=junit" ]] || [[ "$arg" == "--formatter" ]]; then
        HUMAN_OUTPUT=false
    fi
done

echo "Running tests..."

if $HUMAN_OUTPUT; then
    # Run tests with JUnit output to file, then process for human display
    bats --formatter junit "$@" > "$JUNIT_FILE"
    exit_code=$?
    
    # Display human-friendly summary
    python3 /workspace/test-data/junit-summary.py "$JUNIT_FILE"
    
    # Preserve the original exit code
    exit $exit_code
else
    # Just run bats with specified formatter
    bats "$@"
fi