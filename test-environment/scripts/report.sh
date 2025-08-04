#!/bin/bash
# Report Phase - Collect and format test output with ShellSpec results
#
# Enhanced reporting that includes ShellSpec test results and analysis

set -euo pipefail

# Get test result from argument
TEST_RESULT="${1:-UNKNOWN}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo "📊 TEST RESULTS SUMMARY"
echo "======================"

# === JUnit XML Analysis ===
if [ -f "/test-environment/results/junit.xml" ]; then
    echo -e "${GREEN}✅ JUnit XML Results Found${NC}"
    
    # Extract basic stats from JUnit XML
    if command -v xmllint &> /dev/null; then
        TESTS=$(xmllint --xpath 'string(//testsuite/@tests)' /test-environment/results/junit.xml 2>/dev/null || echo "N/A")
        FAILURES=$(xmllint --xpath 'string(//testsuite/@failures)' /test-environment/results/junit.xml 2>/dev/null || echo "N/A")
        ERRORS=$(xmllint --xpath 'string(//testsuite/@errors)' /test-environment/results/junit.xml 2>/dev/null || echo "N/A")
        TIME=$(xmllint --xpath 'string(//testsuite/@time)' /test-environment/results/junit.xml 2>/dev/null || echo "N/A")
        
        echo "📋 JUnit Summary:"
        echo "  Tests: $TESTS"
        echo "  Failures: $FAILURES" 
        echo "  Errors: $ERRORS"
        echo "  Time: ${TIME}s"
        
        # Show failed tests if any
        if [ "$FAILURES" != "0" ] && [ "$FAILURES" != "N/A" ]; then
            echo
            echo -e "${RED}❌ Failed Tests:${NC}"
            echo "----------------"
            xmllint --xpath '//testcase[failure]/@name' /test-environment/results/junit.xml 2>/dev/null | sed 's/ name="/\n  /g; s/"//g' | grep -v '^$' || echo "  (Could not extract failure details)"
        fi
    fi
    echo
else
    echo -e "${YELLOW}⚠️  No JUnit XML results found${NC}"
fi

# === Dodot Log Analysis ===
echo "=== 📝 DODOT LOG ANALYSIS ==="
DODOT_LOG="/home/testuser/.local/share/dodot/dodot.log"
if [ -f "$DODOT_LOG" ]; then
    echo -e "${GREEN}✅ Found dodot log file${NC}"
    LOG_SIZE=$(wc -c < "$DODOT_LOG")
    LOG_LINES=$(wc -l < "$DODOT_LOG")
    echo "📊 Log Stats: $LOG_LINES lines, $LOG_SIZE bytes"
    
    # Show error/warning counts
    ERROR_COUNT=$(grep -i "error" "$DODOT_LOG" | wc -l || echo "0")
    WARN_COUNT=$(grep -i "warn" "$DODOT_LOG" | wc -l || echo "0")
    
    if [ "$ERROR_COUNT" -gt 0 ]; then
        echo -e "  ${RED}Errors: $ERROR_COUNT${NC}"
    fi
    if [ "$WARN_COUNT" -gt 0 ]; then
        echo -e "  ${YELLOW}Warnings: $WARN_COUNT${NC}"
    fi
    
    echo
    echo "📋 Last 10 log entries:"
    echo "----------------------"
    tail -10 "$DODOT_LOG" || echo "(No log content)"
    echo "----------------------"
else
    echo -e "${YELLOW}⚠️  No dodot log file found at $DODOT_LOG${NC}"
fi

echo

# === Summary Report ===
echo "=========================================="
echo "           📋 FINAL REPORT"
echo "=========================================="
echo -e "Test Status: ${TEST_RESULT}"
echo "Timestamp: $(date)"
echo "Container: $(hostname)"

# Available reports
echo
echo "📁 Generated Reports:"
if [ -f "/test-environment/results/junit.xml" ]; then
    echo "  ✅ JUnit XML: /test-environment/results/junit.xml"
fi
if [ -f "/tmp/test-results/test-output.log" ]; then
    echo "  ✅ Full output: /tmp/test-results/test-output.log"
fi

echo "=========================================="

# Always exit successfully - reporting should not fail the pipeline
exit 0