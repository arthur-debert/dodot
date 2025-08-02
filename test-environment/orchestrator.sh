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
#
# Machine-readable output format:
# STAGE <PHASE>: <SUCCESS|FAILURE>

set -euo pipefail

echo "=========================================="
echo "     DODOT DOCKER TEST ORCHESTRATOR"
echo "=========================================="
echo

# Phase 1: Setup (build binary)
echo ">>> Phase 1: Setup"
echo "Building dodot binary inside container..."
if /scripts/setup.sh; then
    SETUP_STATUS="SUCCESS"
    echo "✅ Setup completed successfully"
else
    SETUP_STATUS="FAILURE"
    echo "❌ Setup failed"
fi
echo "STAGE SETUP: $SETUP_STATUS"
echo

# Exit early if setup failed
if [ "$SETUP_STATUS" = "FAILURE" ]; then
    exit 1
fi

# Phase 2: Test (run integration tests)
echo ">>> Phase 2: Test"
echo "Running integration tests..."
if /scripts/test.sh; then
    TEST_STATUS="SUCCESS"
    echo "✅ Tests completed"
else
    TEST_STATUS="FAILURE"
    echo "❌ Tests failed"
fi
echo "STAGE TEST: $TEST_STATUS"
echo

# Phase 3: Report (collect output)
echo ">>> Phase 3: Report"
echo "Collecting test results and logs..."
if /scripts/report.sh "$TEST_STATUS"; then
    REPORT_STATUS="SUCCESS"
else
    REPORT_STATUS="FAILURE"
fi
echo "STAGE REPORT: $REPORT_STATUS"

# Summary for machine parsing
echo
echo "=== FINAL STATUS ==="
echo "STAGE SETUP: $SETUP_STATUS"
echo "STAGE TEST: $TEST_STATUS"
echo "STAGE REPORT: $REPORT_STATUS"

# Exit with appropriate code
# For now, success depends only on setup (as requested)
if [ "$SETUP_STATUS" = "SUCCESS" ]; then
    exit 0
else
    exit 1
fi