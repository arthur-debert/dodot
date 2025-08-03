#!/bin/bash
# Test Phase - Run integration tests using ShellSpec
#
# This script runs the ShellSpec test suite for dodot integration testing

set -euo pipefail

echo "=== Test Phase: Running Integration Tests with ShellSpec ==="
echo

# Verify the container-built binary exists
DODOT="/usr/local/bin/dodot-container-linux"

if [ ! -x "$DODOT" ]; then
    echo "❌ ERROR: dodot-container-linux binary not found"
    echo "Did setup.sh run successfully?"
    exit 1
fi

echo "✅ Found dodot binary at: $DODOT"
echo

# Create results directory for test output
mkdir -p /tmp/test-results/results

# Change to test environment directory where .shellspec is located
cd /test-environment

# Check if ShellSpec is installed
if ! command -v shellspec &> /dev/null; then
    echo "❌ ERROR: ShellSpec not found"
    echo "Make sure the Docker image was rebuilt with ShellSpec installed"
    exit 1
fi

echo "✅ ShellSpec is installed"
echo

# Run ShellSpec tests
echo "Running ShellSpec test suite..."
echo "================================"

# Run tests and capture exit code
set +e
shellspec 2>&1 | tee /tmp/test-results/test-output.log
TEST_EXIT_CODE=${PIPESTATUS[0]}
set -e

echo
echo "================================"

# Copy test results to a location that will be preserved
if [ -f "results/junit.xml" ]; then
    cp results/junit.xml /tmp/test-results/
    echo "✅ JUnit XML results saved to /tmp/test-results/junit.xml"
fi

# Exit with the test exit code
if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo "✅ All tests passed!"
else
    echo "❌ Some tests failed (exit code: $TEST_EXIT_CODE)"
fi

exit $TEST_EXIT_CODE