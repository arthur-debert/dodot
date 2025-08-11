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

# Safety check: ONLY run tests inside Docker container
# Multiple checks to ensure we're in the container
if [ ! -f "/.dockerenv" ] && [ ! -f "/run/.containerenv" ]; then
    echo -e "${RED}ERROR: TESTS MUST RUN INSIDE DOCKER CONTAINER${NC}"
    echo -e "${RED}Running tests outside container could damage your system!${NC}"
    echo ""
    echo "Please use: ./containers/dev/run-tests.sh"
    echo ""
    echo "These tests modify HOME directories and system files."
    echo "They MUST be run in a sandboxed environment."
    exit 1
fi

# Additional check for our specific container
if [ ! -f "/workspace/bin/dodot" ] && [ ! -f "/workspace/scripts/build" ]; then
    echo -e "${RED}ERROR: Not running in dodot development container${NC}"
    echo "Please run: ./containers/dev/run-tests.sh"
    exit 1
fi

# Set a marker that we're in a safe test environment
export DODOT_TEST_CONTAINER=1

# Bats is pre-installed in the container
if ! command -v bats >/dev/null 2>&1; then
    echo -e "${RED}ERROR: Bats is not installed in the container${NC}"
    echo "Please rebuild the container with: containers/dev/build.sh"
    exit 1
fi

# Build dodot once at the start
echo "Ensuring dodot is built..."
if [ -x "/workspace/bin/dodot" ]; then
    echo "dodot already built"
    export PATH="/workspace/bin:$PATH"
elif [ -f "/workspace/scripts/build" ]; then
    echo "Building dodot..."
    /workspace/scripts/build || {
        echo -e "${RED}ERROR: Failed to build dodot${NC}"
        exit 1
    }
    export PATH="/workspace/bin:$PATH"
else
    echo -e "${RED}ERROR: Cannot find build script${NC}"
    exit 1
fi

# Find all test scenarios
SCENARIOS=()

# Suite 1 has a special structure with subdirectories for each power-up
if [ -d "scenarios/suite-1-single-powerups" ]; then
    SCENARIOS+=("scenarios/suite-1-single-powerups")
fi

# Suites 2-5 have a standard structure with a tests directory
for suite_dir in scenarios/suite-[2-5]*; do
    if [ -d "$suite_dir/tests" ]; then
        SCENARIOS+=("$suite_dir")
    fi
done

# Test framework scenario
if [ -d "scenarios/test-framework/tests" ]; then
    SCENARIOS+=("scenarios/test-framework")
fi

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
    echo ""  # Add spacing between scenarios
    
    # Find all .bats files in the scenario
    if [ "$scenario_name" = "suite-1-single-powerups" ]; then
        # Suite 1 has subdirectories for each power-up
        bats_files=()
        for powerup_dir in "$scenario"/*/tests; do
            if [ -d "$powerup_dir" ]; then
                for bats_file in "$powerup_dir"/*.bats; do
                    if [ -f "$bats_file" ]; then
                        bats_files+=("$bats_file")
                    fi
                done
            fi
        done
    else
        # Standard structure with tests directory
        bats_files=("$scenario/tests"/*.bats)
    fi
    
    if [ ${#bats_files[@]} -eq 0 ] || [ ! -f "${bats_files[0]}" ]; then
        continue
    fi
    
    # Run bats tests - always show full output
    echo -e "${YELLOW}Running scenario: $scenario_name${NC}"
    
    if $FAILFAST; then
        # Run tests one by one for failfast behavior
        test_failed=false
        for bats_file in "${bats_files[@]}"; do
            set +e
            bats "$bats_file"
            bats_exit_code=$?
            set -e
            
            if [ $bats_exit_code -ne 0 ]; then
                test_failed=true
                break
            fi
        done
        if $test_failed; then
            echo -e "  ${RED}✗ Some tests failed${NC}"
            FAILED_SCENARIOS+=("$scenario_name")
            FAILED_TESTS=$((FAILED_TESTS + 1))
            echo ""
            echo -e "${RED}Failing fast as requested${NC}"
            exit 1
        else
            echo -e "  ${GREEN}✓ All tests passed${NC}"
        fi
    else
        # Normal mode - run all tests at once
        # Temporarily disable set -e to handle bats exit code
        set +e
        bats "${bats_files[@]}"
        bats_exit_code=$?
        set -e
        
        if [ $bats_exit_code -eq 0 ]; then
            echo -e "  ${GREEN}✓ All tests passed${NC}"
        else
            echo -e "  ${RED}✗ Some tests failed${NC}"
            FAILED_SCENARIOS+=("$scenario_name")
            FAILED_TESTS=$((FAILED_TESTS + 1))
        fi
    fi
    echo ""
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
done

# Summary
echo ""
echo "========================================="
echo "TEST SUMMARY" 
echo "========================================="
echo "Total scenarios run: ${#SCENARIOS[@]}"
echo ""

if [ ${#FAILED_SCENARIOS[@]} -eq 0 ]; then
    echo -e "${GREEN}All scenarios passed!${NC}"
    echo ""
    for scenario in "${SCENARIOS[@]}"; do
        echo -e "  ${GREEN}✓${NC} $(basename "$scenario")"
    done
    exit 0
else
    echo -e "${RED}${#FAILED_SCENARIOS[@]} scenario(s) failed:${NC}"
    echo ""
    for failed in "${FAILED_SCENARIOS[@]}"; do
        echo -e "  ${RED}✗${NC} $failed"
    done
    echo ""
    echo "Passed scenarios:"
    for scenario in "${SCENARIOS[@]}"; do
        scenario_name=$(basename "$scenario")
        if [[ ! " ${FAILED_SCENARIOS[@]} " =~ " ${scenario_name} " ]]; then
            echo -e "  ${GREEN}✓${NC} $scenario_name"
        fi
    done
    exit 1
fi