#!/bin/bash
# Run dodot live system tests using Bats native formatters
#
# This script runs tests in a Docker container and leverages Bats' built-in
# formatting capabilities. All arguments are passed directly to Bats.

set -e

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Show help
show_help() {
    cat << EOF
Usage: $0 [OPTIONS] [TEST_PATTERNS...]

Run dodot live system tests using Bats native formatters.

OPTIONS:
    -h, --help              Show this help message
    --formatter FORMAT      Use specific Bats formatter (pretty, tap, tap13, junit)
    --filter REGEX          Only run tests matching regex pattern
    -t, --timing            Add timing information
    --verbose               Show output of passing tests
    
TEST_PATTERNS:
    If no patterns specified, runs all tests (test-data/scenarios/**/*.bats)
    
    Examples:
      # Run all tests (default)
      $0
      
      # Run specific suite
      $0 test-data/scenarios/suite-1-single-powerups/**/*.bats
      
      # Run specific test file
      $0 test-data/scenarios/suite-1-single-powerups/path/tests/path.bats
      
      # Run tests matching pattern
      $0 --filter "path: YES"
      
      # Generate JUnit XML output (saved to test-results.xml in project root)
      $0 --formatter junit
      
      # Run with timing information
      $0 --timing

All options are passed directly to Bats. See 'bats --help' for more options.

The JUnit XML report (if generated) is saved to /workspace/test-results.xml
and is accessible from the host for CI processing.
EOF
    exit 0
}

# Check for help flag
for arg in "$@"; do
    if [[ "$arg" == "-h" ]] || [[ "$arg" == "--help" ]]; then
        show_help
    fi
done

echo "Running dodot live system tests..."
echo ""

# Function to run command with progress indicator
run_with_progress() {
    local desc="$1"
    local cmd="$2"
    
    printf "%-40s" "$desc..."
    
    if eval "$cmd" >/dev/null 2>&1; then
        echo -e "${GREEN}✓${NC}"
    else
        echo -e "${RED}✗${NC}"
        echo -e "${RED}Error during: $desc${NC}"
        return 1
    fi
}

# First build dodot in the container
run_with_progress "Building dodot" '"$SCRIPT_DIR/run.sh" ./scripts/build'

echo ""
echo "Running tests..."
echo "============================================="
echo ""

# Run the test suite with native runner
# The runner handles JUnit output and human-friendly display
# Pass arguments carefully to preserve quoting
exec "$SCRIPT_DIR/run.sh" /workspace/test-data/runner.sh "$@"

# Exit code is preserved from the test run
exit_code=$?

if [ $exit_code -eq 0 ]; then
    echo ""
    echo -e "${GREEN}All tests passed!${NC}"
else
    echo ""
    echo -e "${RED}Some tests failed!${NC}"
fi

# Note about JUnit output
if [[ "$*" == *"--formatter=junit"* ]] || [[ "$*" == *"--formatter junit"* ]]; then
    echo ""
    echo "JUnit XML report saved to: $(pwd)/test-results.xml"
fi

exit $exit_code