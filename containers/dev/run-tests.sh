#!/bin/bash
# Convenience script to run dodot live system tests in container

set -e

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Parse arguments
VERBOSE=false
if [[ "$1" == "-v" ]] || [[ "$1" == "--verbose" ]]; then
    VERBOSE=true
fi

echo "Running dodot live system tests..."
echo ""

# Function to run command with progress indicator
run_with_progress() {
    local desc="$1"
    local cmd="$2"
    local log_file="/tmp/dodot-test-$$-$(echo "$desc" | tr ' ' '-').log"
    
    printf "%-40s" "$desc..."
    
    if $VERBOSE; then
        # In verbose mode, show all output
        if eval "$cmd"; then
            echo -e "${GREEN}✓${NC}"
        else
            echo -e "${RED}✗${NC}"
            return 1
        fi
    else
        # In quiet mode, capture output
        if eval "$cmd" > "$log_file" 2>&1; then
            echo -e "${GREEN}✓${NC}"
            rm -f "$log_file"
        else
            echo -e "${RED}✗${NC}"
            echo -e "${RED}Error during: $desc${NC}"
            echo "Full output:"
            cat "$log_file"
            rm -f "$log_file"
            return 1
        fi
    fi
}

# First build dodot in the container
run_with_progress "Building dodot" '"$SCRIPT_DIR/run.sh" ./scripts/build'

echo ""
# Run the test suite
echo "Running tests..."
if $VERBOSE; then
    "$SCRIPT_DIR/run.sh" /workspace/test-data/runner.sh --verbose
else
    "$SCRIPT_DIR/run.sh" /workspace/test-data/runner.sh
fi