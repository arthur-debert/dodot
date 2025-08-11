#!/bin/bash
# Run tests with human-friendly output
#
# This script runs tests with JUnit formatter and pipes to Python formatter

set -e

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Show help
show_help() {
    cat << EOF
Usage: $0 [TEST_FILES...]

Run dodot live system tests with human-friendly output.

This script:
1. Runs tests with JUnit formatter
2. Processes the XML output to show suite-grouped results
3. Displays a summary at the end

Examples:
  $0                                              # Run all tests
  $0 test-data/scenarios/suite-1/**/*.bats       # Run specific suite
  $0 test-data/scenarios/**/symlink.bats         # Run specific test

EOF
    exit 0
}

# Check for help
if [[ "$1" == "-h" ]] || [[ "$1" == "--help" ]]; then
    show_help
fi

echo "Running dodot live system tests..."
echo "================================="
echo ""

# Create temp file for XML output
JUNIT_FILE=$(mktemp)
trap "rm -f $JUNIT_FILE" EXIT

# Run tests with JUnit output, capture to temp file
if "$SCRIPT_DIR/run-tests.sh" --formatter junit "$@" > "$JUNIT_FILE"; then
    exit_code=0
else
    exit_code=$?
fi

# Process the JUnit output for human display
echo "" >&2
python3 "$PROJECT_ROOT/test-data/junit-summary.py" "$JUNIT_FILE"

# Exit with the test exit code
exit $exit_code