#!/usr/bin/env bash
# Main test runner for dodot live system tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# No arguments needed for now

echo "========================================="
echo "dodot Live System Tests"
echo "========================================="
echo ""

# Find test root
TEST_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$TEST_ROOT"

# Check if running in container
if [ ! -f "/workspace/bin/dodot" ] && [ ! -f "/workspace/scripts/build" ]; then
    echo -e "${RED}ERROR: Not running in dodot development container${NC}"
    echo "Please run: containers/dev/run.sh $0"
    exit 1
fi

# Ensure Bats is available
if ! command -v bats >/dev/null 2>&1; then
    printf "%-40s" "Installing Bats..."
    if sudo apt-get update >/dev/null 2>&1 && sudo apt-get install -y bats >/dev/null 2>&1; then
        echo -e "${GREEN}✓${NC}"
    else
        echo -e "${RED}✗${NC}"
        echo -e "${RED}Failed to install Bats${NC}"
        exit 1
    fi
fi

# Find all test scenarios
SCENARIOS=()
for scenario in scenarios/*/tests; do
    if [ -d "$scenario" ]; then
        SCENARIOS+=("${scenario%/tests}")
    fi
done

if [ ${#SCENARIOS[@]} -eq 0 ]; then
    echo -e "${RED}No test scenarios found${NC}"
    exit 1
fi

echo "Found ${#SCENARIOS[@]} test scenario(s):"
for scenario in "${SCENARIOS[@]}"; do
    echo "  - $(basename "$scenario")"
done
echo ""

# Run tests for each scenario
TOTAL_TESTS=0
FAILED_TESTS=0
FAILED_SCENARIOS=()

for scenario in "${SCENARIOS[@]}"; do
    scenario_name=$(basename "$scenario")
    
    # Find all .bats files in the scenario
    bats_files=("$scenario/tests"/*.bats)
    
    if [ ${#bats_files[@]} -eq 0 ] || [ ! -f "${bats_files[0]}" ]; then
        continue
    fi
    
    # Run bats tests - always show full output
    echo -e "${YELLOW}Running scenario: $scenario_name${NC}"
    if bats "${bats_files[@]}"; then
        echo -e "  ${GREEN}✓ All tests passed${NC}"
    else
        echo -e "  ${RED}✗ Some tests failed${NC}"
        FAILED_SCENARIOS+=("$scenario_name")
        ((FAILED_TESTS++))
    fi
    echo ""
    ((TOTAL_TESTS++))
done

# Summary
echo "========================================="
echo "Test Summary"
echo "========================================="
if [ $FAILED_TESTS -eq 0 ]; then
    echo -e "${GREEN}All scenarios passed!${NC}"
    echo "Total scenarios: $TOTAL_TESTS"
    exit 0
else
    echo -e "${RED}$FAILED_TESTS scenario(s) failed${NC}"
    echo "Total scenarios: $TOTAL_TESTS"
    exit 1
fi