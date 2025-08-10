#!/bin/bash
# Run tests and show just the summary
set -e

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Running dodot live system tests (summary mode)..."
echo ""

# Build dodot first
"$SCRIPT_DIR/run.sh" ./scripts/build >/dev/null 2>&1 || {
    echo "Failed to build dodot"
    exit 1
}
echo "✓ dodot built"

# Run tests and capture key output
"$SCRIPT_DIR/run.sh" /workspace/test-data/runner.sh 2>&1 | grep -E "(Found|Running scenario:|ok |not ok |skip|passed|failed|TEST SUMMARY|Total scenarios|✓|✗)" | sed 's/^/  /'

echo ""
echo "For full output, run: ./containers/dev/run-tests.sh"