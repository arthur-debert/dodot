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

# Run ShellSpec tests with enhanced reporting
echo "Running ShellSpec test suite..."
echo "================================"

# Run tests and capture exit code
# Use ShellSpec's built-in formatters for better reporting
set +e
shellspec \
    --format progress \
    --format junit \
    --format tap \
    --output-junit /tmp/test-results/junit.xml \
    --output-tap /tmp/test-results/test-results.tap \
    2>&1 | tee /tmp/test-results/test-output.log
TEST_EXIT_CODE=${PIPESTATUS[0]}
set -e

echo
echo "================================"
echo "=== TEST SUMMARY ==="

# Display test summary if available
if [ -f "/tmp/test-results/test-results.tap" ]; then
    echo
    echo "📊 TAP Test Results:"
    echo "-------------------"
    cat /tmp/test-results/test-results.tap | head -20
    echo
    
    # Count test results
    TOTAL_TESTS=$(grep -c "^ok\|^not ok" /tmp/test-results/test-results.tap || echo "0")
    PASSED_TESTS=$(grep -c "^ok" /tmp/test-results/test-results.tap || echo "0")
    FAILED_TESTS=$(grep -c "^not ok" /tmp/test-results/test-results.tap || echo "0")
    
    echo "📈 Test Statistics:"
    echo "  Total:  $TOTAL_TESTS"
    echo "  Passed: $PASSED_TESTS"
    echo "  Failed: $FAILED_TESTS"
fi

# Show files created
echo
echo "📁 Generated Reports:"
if [ -f "/tmp/test-results/junit.xml" ]; then
    echo "  ✅ JUnit XML: /tmp/test-results/junit.xml"
fi
if [ -f "/tmp/test-results/test-results.tap" ]; then
    echo "  ✅ TAP format: /tmp/test-results/test-results.tap"
fi
if [ -f "/tmp/test-results/test-output.log" ]; then
    echo "  ✅ Full output: /tmp/test-results/test-output.log"
fi

# Exit with the test exit code
if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo "✅ All tests passed!"
else
    echo "❌ Some tests failed (exit code: $TEST_EXIT_CODE)"
fi

exit $TEST_EXIT_CODE