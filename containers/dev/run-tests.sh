#!/bin/bash
# Run dodot live system tests - passes all arguments to Bats
#
# Usage: ./run-tests.sh [BATS_OPTIONS] [TEST_FILES...]
# 
# All arguments are passed directly to Bats inside the container.
# For human-friendly output, use run-tests-friendly-summary.sh

set -e

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Show help
show_help() {
    cat << EOF
Usage: $0 [BATS_OPTIONS] [TEST_FILES...]

Run dodot live system tests by passing all arguments directly to Bats.

This script:
1. Builds dodot in the container
2. Runs Bats with your specified options
3. Outputs results to stdout (format depends on --formatter option)

Common usage:
  $0                           # Run all tests with default formatter
  $0 --formatter tap           # Output TAP format
  $0 --formatter junit         # Output JUnit XML
  $0 test-data/scenarios/suite-1/**/*.bats  # Run specific tests

For human-friendly output with suite grouping, use:
  ./run-tests-friendly-summary.sh

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

# Build dodot first (suppress output)
echo "Building dodot..." >&2
if ! "$SCRIPT_DIR/run.sh" ./scripts/build >/dev/null 2>&1; then
    echo -e "${RED}✗ Build failed${NC}" >&2
    exit 1
fi
echo -e "${GREEN}✓ Build successful${NC}" >&2
echo "" >&2

# Run tests - pass all args to runner.sh
exec "$SCRIPT_DIR/run.sh" /workspace/test-data/runner.sh "$@"