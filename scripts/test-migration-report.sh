#!/bin/bash

# Script to generate a comprehensive test migration coverage report
# This script runs coverage analysis on both old and new tests separately

set -e

echo "Test Migration Coverage Report"
echo "=============================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check prerequisites
if ! command_exists go; then
    echo -e "${RED}Error: Go is not installed${NC}"
    exit 1
fi

if ! command_exists python3; then
    echo -e "${RED}Error: Python3 is not installed${NC}"
    exit 1
fi

# Parse command line arguments
PACKAGE=""
SKIP_RUN=false
VERBOSE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --package)
            PACKAGE="$2"
            shift 2
            ;;
        --skip-run)
            SKIP_RUN=true
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --package PKG    Analyze only a specific package"
            echo "  --skip-run       Skip test execution, use existing coverage files"
            echo "  --verbose        Show detailed output"
            echo "  -h, --help       Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Step 1: Run the comparison tool
echo -e "${GREEN}Running test migration coverage analysis...${NC}"
CMD="./scripts/test-migration-coverage"
if [ ! -z "$PACKAGE" ]; then
    CMD="$CMD --package $PACKAGE"
fi
if [ "$SKIP_RUN" = true ]; then
    CMD="$CMD --skip-old --skip-new"
fi

if [ "$VERBOSE" = true ]; then
    echo "Command: $CMD"
fi

$CMD

# Step 2: Generate detailed summaries if coverage files exist
echo ""
if [ -f "coverage_old.out" ]; then
    echo -e "${YELLOW}Detailed summary for old tests (_toremove_test.go):${NC}"
    ./scripts/coverage-summary coverage_old.out --title "Old Tests Coverage"
fi

if [ -f "coverage_new.out" ]; then
    echo -e "${YELLOW}Detailed summary for new tests (_test.go):${NC}"
    ./scripts/coverage-summary coverage_new.out --title "New Tests Coverage"
fi

# Step 3: Show packages with low coverage that need attention
echo ""
echo -e "${YELLOW}Packages needing attention (coverage < 60%):${NC}"
if [ -f "coverage_new.out" ]; then
    ./scripts/coverage-summary coverage_new.out --title "Low Coverage Packages (New Tests)" --max-coverage 60
fi

# Step 4: Summary statistics
echo ""
echo -e "${GREEN}Migration Statistics:${NC}"
NEW_COUNT=$(find . -name "*_test.go" -not -name "*_toremove_test.go" | wc -l)
OLD_COUNT=$(find . -name "*_toremove_test.go" | wc -l)
TOTAL_COUNT=$((NEW_COUNT + OLD_COUNT))
if [ $TOTAL_COUNT -gt 0 ]; then
    MIGRATION_PCT=$(echo "scale=2; $NEW_COUNT * 100 / $TOTAL_COUNT" | bc)
else
    MIGRATION_PCT=0
fi

echo "New test files: $NEW_COUNT"
echo "Old test files: $OLD_COUNT"
echo "Migration progress: ${MIGRATION_PCT}%"

echo ""
echo "Report generation complete!"