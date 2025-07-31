#!/bin/bash
# Edge case tests for dodot
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
    echo -e "${YELLOW}EDGE CASE TEST: $1${NC}"
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
    # Remove any test artifacts
    rm -rf ~/.vimrc ~/.zshrc ~/.gitconfig ~/.ssh ~/test-*
    rm -rf /tmp/dodot-test-*
}

# Start tests
echo "=== Dodot Edge Case Tests ==="
echo
cleanup

# TEST 1: Broken symlink handling
test_start "Handles broken symlinks gracefully"
ln -s /nonexistent/file ~/.vimrc
if dodot deploy vim 2>&1 | grep -q "Error"; then
    test_pass
else
    test_fail "Should handle broken symlink"
fi

# TEST 2: Read-only file conflict
test_start "Handles read-only file conflicts"
cleanup
echo "readonly" > ~/.vimrc
chmod 444 ~/.vimrc
if ! dodot deploy vim 2> /dev/null; then
    test_pass
else
    test_fail "Should fail with read-only file"
fi
chmod 644 ~/.vimrc
rm ~/.vimrc

# TEST 3: Directory where file expected
test_start "Handles directory/file type mismatch"
cleanup
mkdir ~/.vimrc
if ! dodot deploy vim 2> /dev/null; then
    test_pass
else
    test_fail "Should fail when directory exists instead of file"
fi
rmdir ~/.vimrc

# TEST 4: Very long path handling
test_start "Handles very long paths"
cleanup
# Create a deeply nested structure
LONGPATH="$HOME/test"
for i in {1..20}; do
    LONGPATH="$LONGPATH/very_long_directory_name_$i"
done
mkdir -p "$(dirname "$LONGPATH")"
# This should complete without hanging or crashing
if dodot list > /dev/null 2>&1; then
    test_pass
else
    test_fail "Failed with long paths"
fi
rm -rf ~/test

# TEST 5: Unicode in filenames
test_start "Handles unicode filenames"
cleanup
# Create a pack with unicode
mkdir -p /tmp/dodot-test-unicode/test-émojis-🎉
cat > /tmp/dodot-test-unicode/test-émojis-🎉/pack.dodot.toml << 'EOF'
name = "test-émojis-🎉"
[[matchers]]
triggers = [{ type = "FileName", pattern = "test-文件.txt" }]
actions = [{ type = "Symlink" }]
EOF
touch "/tmp/dodot-test-unicode/test-émojis-🎉/test-文件.txt"
# Test with unicode pack
DOTFILES_ROOT=/tmp/dodot-test-unicode dodot list 2>&1 | grep -q "test-émojis" && test_pass || test_fail "Unicode handling failed"

# TEST 6: Circular symlink detection
test_start "Detects circular symlinks"
cleanup
ln -s ~/.vimrc ~/.vimrc
if ! dodot deploy vim 2> /dev/null; then
    test_pass
else
    test_fail "Should detect circular symlink"
fi
rm ~/.vimrc

# TEST 7: No write permissions in target
test_start "Handles no write permissions in target directory"
cleanup
mkdir -p ~/test-readonly
chmod 555 ~/test-readonly
# Try to deploy something that would write there
if ! HOME=~/test-readonly dodot deploy vim 2> /dev/null; then
    test_pass
else
    test_fail "Should fail with no write permissions"
fi
chmod 755 ~/test-readonly
rm -rf ~/test-readonly

# TEST 8: Symlink to symlink scenario
test_start "Handles symlink chains correctly"
cleanup
# Create a chain: .vimrc -> .vimrc.bak -> actual file
echo "actual content" > ~/.vimrc.actual
ln -s ~/.vimrc.actual ~/.vimrc.bak
ln -s ~/.vimrc.bak ~/.vimrc
# Deploy should replace the chain
dodot deploy vim > /dev/null 2>&1
if [ -L ~/.vimrc ] && readlink ~/.vimrc | grep -q "/dotfiles/vim/.vimrc"; then
    test_pass
else
    test_fail "Failed to handle symlink chain"
fi

# TEST 9: Pack with no matchers
test_start "Handles pack with empty matchers"
mkdir -p /tmp/dodot-test-empty/empty-pack
cat > /tmp/dodot-test-empty/empty-pack/pack.dodot.toml << 'EOF'
name = "empty-pack"
# No matchers defined
EOF
DOTFILES_ROOT=/tmp/dodot-test-empty dodot deploy empty-pack > /dev/null 2>&1 && test_pass || test_fail "Failed with empty pack"

# TEST 10: Concurrent deployment safety
test_start "Handles concurrent operations safely"
cleanup
# Run multiple deploys in background
dodot deploy vim &
PID1=$!
dodot deploy zsh &
PID2=$!
dodot deploy git &
PID3=$!

# Wait for all to complete
wait $PID1 $PID2 $PID3

# Check all deployed correctly
if [ -L ~/.vimrc ] && [ -L ~/.zshrc ] && [ -L ~/.gitconfig ]; then
    test_pass
else
    test_fail "Concurrent deployment issues"
fi

# Summary
echo "=== Edge Case Test Summary ==="
echo "Tests run: $TESTS_RUN"
echo "Tests passed: $TESTS_PASSED"
echo

if [ $TESTS_PASSED -eq $TESTS_RUN ]; then
    echo -e "${GREEN}All edge case tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some edge case tests failed!${NC}"
    exit 1
fi