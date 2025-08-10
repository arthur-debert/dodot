#!/usr/bin/env bash
# Temporary runner for suite 2 only

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "========================================="
echo "dodot Live System Tests - Suite 2 Only"
echo "========================================="
echo ""

# Safety check
if [ ! -f "/.dockerenv" ] && [ ! -f "/run/.containerenv" ]; then
    echo -e "${RED}ERROR: TESTS MUST RUN INSIDE DOCKER CONTAINER${NC}"
    exit 1
fi

if [ -z "$DODOT_TEST_CONTAINER" ] || [ "$DODOT_TEST_CONTAINER" != "1" ]; then
    echo -e "${RED}ERROR: Not in test container environment${NC}"
    exit 1
fi

# Ensure dodot is built
echo "Ensuring dodot is built..."
if [ ! -f "/workspace/bin/dodot" ]; then
    echo -e "${RED}ERROR: dodot binary not found${NC}"
    exit 1
fi
echo "dodot already built"

# Run only suite 2
cd /workspace/test-data
echo -e "${YELLOW}Running scenario: suite-2-multi-powerups-single-pack${NC}"
if bats scenarios/suite-2-multi-powerups-single-pack/tests/*.bats; then
    echo -e "  ${GREEN}✓ All tests passed${NC}"
else
    echo -e "  ${RED}✗ Some tests failed${NC}"
    exit 1
fi