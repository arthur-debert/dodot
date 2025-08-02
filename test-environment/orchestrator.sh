#!/bin/bash
# Orchestrator script - Stable entry point for Docker container
# 
# IMPORTANT: This script should rarely change. It simply calls the three phases
# in order. All actual work is done in the phase scripts, making it easy to
# modify behavior without touching Docker configuration.
#
# The three phases are:
# 1. setup.sh - Build dodot binary inside container
# 2. test.sh - Run integration tests
# 3. report.sh - Collect and format output

set -euo pipefail

echo "=========================================="
echo "     DODOT DOCKER TEST ORCHESTRATOR"
echo "=========================================="
echo

# Phase 1: Setup (build binary)
echo ">>> Phase 1: Setup"
echo "Building dodot binary inside container..."
if /scripts/setup.sh; then
    echo "✅ Setup completed successfully"
else
    echo "❌ Setup failed"
    exit 1
fi
echo

# Phase 2: Test (run integration tests)
echo ">>> Phase 2: Test"
echo "Running integration tests..."
if /scripts/test.sh; then
    TEST_RESULT="PASSED"
    echo "✅ Tests completed"
else
    TEST_RESULT="FAILED"
    echo "❌ Tests failed"
fi
echo

# Phase 3: Report (collect output)
echo ">>> Phase 3: Report"
echo "Collecting test results and logs..."
/scripts/report.sh "$TEST_RESULT"

# Exit with appropriate code
if [ "$TEST_RESULT" = "PASSED" ]; then
    exit 0
else
    exit 1
fi