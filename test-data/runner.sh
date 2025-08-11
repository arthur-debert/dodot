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
    echo "Building dodot..." >&2
    /workspace/scripts/build >&2 || {
        echo "ERROR: Failed to build dodot" >&2
        exit 1
    }
fi
export PATH="/workspace/bin:$PATH"

# Check if any of the args are test files/directories
has_test_files=false
for arg in "$@"; do
    if [[ "$arg" == *.bats ]] || [[ -d "$arg" ]] || [[ -f "$arg" ]]; then
        has_test_files=true
        break
    fi
done

# If no test files in args, find and append all test files
if [ "$has_test_files" = false ]; then
    # Find all test files
    test_files=()
    while IFS= read -r -d '' file; do
        test_files+=("$file")
    done < <(find /workspace/test-data/scenarios -name "*.bats" -type f -print0 | sort -z)
    
    if [ ${#test_files[@]} -eq 0 ]; then
        echo "ERROR: No test files found" >&2
        exit 1
    fi
    
    # Append test files to existing args (like --formatter junit)
    set -- "$@" "${test_files[@]}"
fi

# Just run bats with whatever args we have
exec bats "$@"