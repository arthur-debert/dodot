#!/usr/bin/env bash
# Main test runner for dodot live system tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Parse arguments
FAILFAST=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --failfast)
            FAILFAST=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--failfast]"
            exit 1
            ;;
    esac
done

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

# Bats is pre-installed in the container
if ! command -v bats >/dev/null 2>&1; then
    echo -e "${RED}ERROR: Bats is not installed in the container${NC}"
    echo "Please rebuild the container with: containers/dev/build.sh"
    exit 1
fi

# Find all test scenarios
SCENARIOS=()
# Look for old-style scenarios with tests directory
for scenario in scenarios/*/tests; do
    if [ -d "$scenario" ]; then
        SCENARIOS+=("${scenario%/tests}")
    fi
done
# Look for new suite structure
for suite in scenarios/suite-*/tests; do
    if [ -d "$suite" ]; then
        SCENARIOS+=("${suite%/tests}")
    fi
done
# Look for power-up specific tests in Suite 1
for powerup in scenarios/suite-1-single-powerups/*/tests; do
    if [ -d "$powerup" ]; then
        SCENARIOS+=("${powerup%/tests}")
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
    
    if $FAILFAST; then
        # Run tests one by one for failfast behavior
        test_failed=false
        for bats_file in "${bats_files[@]}"; do
            if ! bats "$bats_file"; then
                test_failed=true
                break
            fi
        done
        if $test_failed; then
            echo -e "  ${RED}✗ Some tests failed${NC}"
            FAILED_SCENARIOS+=("$scenario_name")
            ((FAILED_TESTS++))
            echo ""
            echo -e "${RED}Failing fast as requested${NC}"
            exit 1
        else
            echo -e "  ${GREEN}✓ All tests passed${NC}"
        fi
    else
        # Normal mode - run all tests at once
        if bats "${bats_files[@]}"; then
            echo -e "  ${GREEN}✓ All tests passed${NC}"
        else
            echo -e "  ${RED}✗ Some tests failed${NC}"
            FAILED_SCENARIOS+=("$scenario_name")
            ((FAILED_TESTS++))
        fi
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