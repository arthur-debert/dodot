#!/bin/bash
# Run dodot live system tests - passes all arguments to Bats
#
# Usage: ./run-tests.sh [BATS_OPTIONS] [TEST_FILES...]
#
# All arguments are passed directly to Bats inside the container.
# Output goes to stdout (format depends on --formatter option)
# For human-friendly output, use run-tests-pretty.sh

set -e

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Show help
show_help() {
    cat <<EOF
Usage: $0 [BATS_OPTIONS] [TEST_FILES...]

Run dodot live system tests by passing all arguments directly to Bats.

This script:
1. Builds dodot in the container
2. Runs Bats with your specified options
3. Outputs results to stdout (format depends on --formatter option)

Common usage:
  $0                                        # Run all tests with default formatter
  $0 --formatter tap                        # Output TAP format
  $0 --formatter junit                      # Output JUnit XML
  $0 live-testing/scenarios/suite-1/**/*.bats # Run specific suite
  $0 live-testing/scenarios/**/symlink.bats   # Run specific test file

For human-friendly output with suite grouping, use:
  ./run-tests-pretty.sh

All Bats options are supported. See 'bats --help' for details.
EOF
    exit 0
}

# Check for help flag
for arg in "$@"; do
    if [[ "$arg" == "-h" ]] || [[ "$arg" == "--help" ]]; then
        show_help
    fi
done

# # Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Run the runner.sh in base docker container (same as CI), passing all args
exec "$SCRIPT_DIR/run-base.sh" /workspace/live-testing/scripts/runner.sh "$@"
