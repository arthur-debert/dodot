#!/bin/bash
# Run all dodot integration tests
set -euo pipefail

echo "=========================================="
echo "     DODOT INTEGRATION TEST SUITE"
echo "=========================================="
echo

# First verify setup
echo ">>> Running setup verification..."
/setup-scripts/test-setup.sh
echo

# Run basic tests
echo ">>> Running basic tests..."
if /setup-scripts/run-basic-tests.sh; then
    BASIC_RESULT="PASSED"
else
    BASIC_RESULT="FAILED"
fi
echo

# Run edge case tests
echo ">>> Running edge case tests..."
if /setup-scripts/run-edge-case-tests.sh; then
    EDGE_RESULT="PASSED"
else
    EDGE_RESULT="FAILED"
fi
echo

# Summary
echo "=========================================="
echo "           TEST SUITE SUMMARY"
echo "=========================================="
echo "Basic Tests:     $BASIC_RESULT"
echo "Edge Case Tests: $EDGE_RESULT"
echo

if [ "$BASIC_RESULT" = "PASSED" ] && [ "$EDGE_RESULT" = "PASSED" ]; then
    echo "✅ ALL TESTS PASSED"
    exit 0
else
    echo "❌ SOME TESTS FAILED"
    exit 1
fi