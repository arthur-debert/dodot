#!/bin/bash
# Automated basic tests for dodot
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TESTS_RUN=0
TESTS_PASSED=0

# Test helper functions
test_start() {
    echo -e "${YELLOW}TEST: $1${NC}"
    ((TESTS_RUN++))
}

test_pass() {
    echo -e "${GREEN}✓ PASS${NC}"
    ((TESTS_PASSED++))
    echo
}

test_fail() {
    echo -e "${RED}✗ FAIL: $1${NC}"
    echo
}

# Cleanup function
cleanup() {
    # Remove any symlinks created during tests
    rm -f ~/.vimrc ~/.zshrc ~/.gitconfig ~/.gitignore_global ~/.ssh/config
    rm -rf ~/.ssh
}

# Start tests
echo "=== Dodot Automated Basic Tests ==="
echo
cleanup

# TEST 1: Verify dodot is available
test_start "dodot binary is available"
if command -v dodot &> /dev/null; then
    test_pass
else
    test_fail "dodot command not found"
    exit 1
fi

# TEST 2: List packs
test_start "dodot list shows all packs"
OUTPUT=$(dodot list 2>&1)
if echo "$OUTPUT" | grep -q "vim" && \
   echo "$OUTPUT" | grep -q "zsh" && \
   echo "$OUTPUT" | grep -q "git" && \
   echo "$OUTPUT" | grep -q "ssh"; then
    test_pass
else
    test_fail "Expected packs not found in output"
fi

# TEST 3: Deploy vim pack
test_start "dodot deploy vim creates symlink"
dodot deploy vim > /dev/null 2>&1
if [ -L ~/.vimrc ] && [ -e ~/.vimrc ]; then
    test_pass
else
    test_fail "~/.vimrc symlink not created"
fi

# TEST 4: Verify symlink target
test_start "vim symlink points to correct target"
TARGET=$(readlink ~/.vimrc)
if [[ "$TARGET" == *"/dotfiles/vim/.vimrc" ]]; then
    test_pass
else
    test_fail "Symlink target is incorrect: $TARGET"
fi

# TEST 5: Deploy with dry-run
test_start "dry-run mode doesn't make changes"
cleanup
OUTPUT=$(dodot deploy git --dry-run 2>&1)
if echo "$OUTPUT" | grep -q "DRY RUN MODE" && [ ! -L ~/.gitconfig ]; then
    test_pass
else
    test_fail "Dry run mode failed"
fi

# TEST 6: Deploy multiple packs
test_start "deploy multiple packs at once"
dodot deploy vim zsh > /dev/null 2>&1
if [ -L ~/.vimrc ] && [ -L ~/.zshrc ]; then
    test_pass
else
    test_fail "Multiple pack deployment failed"
fi

# TEST 7: Status shows deployed packs
test_start "status command shows deployment state"
OUTPUT=$(dodot status vim 2>&1)
if echo "$OUTPUT" | grep -q "vim:" && echo "$OUTPUT" | grep -q "Symlink:"; then
    test_pass
else
    test_fail "Status output incorrect"
fi

# TEST 8: Deploy with existing non-symlink file
test_start "deploy fails with existing non-symlink file"
cleanup
echo "manual config" > ~/.vimrc
if ! dodot deploy vim 2> /dev/null; then
    test_pass
else
    test_fail "Should have failed with existing file"
fi

# TEST 9: SSH pack creates directory
test_start "ssh pack creates .ssh directory"
cleanup
dodot deploy ssh > /dev/null 2>&1
if [ -d ~/.ssh ] && [ -L ~/.ssh/config ]; then
    test_pass
else
    test_fail ".ssh directory or config not created"
fi

# TEST 10: All packs deployment
test_start "deploy all packs with no arguments"
cleanup
dodot deploy > /dev/null 2>&1
if [ -L ~/.vimrc ] && [ -L ~/.zshrc ] && [ -L ~/.gitconfig ] && [ -L ~/.ssh/config ]; then
    test_pass
else
    test_fail "Not all packs were deployed"
fi

# Summary
echo "=== Test Summary ==="
echo "Tests run: $TESTS_RUN"
echo "Tests passed: $TESTS_PASSED"
echo

if [ $TESTS_PASSED -eq $TESTS_RUN ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi