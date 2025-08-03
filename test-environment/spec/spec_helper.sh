#!/bin/zsh
# ShellSpec helper file for common test setup

# Set up test environment
export DODOT="/usr/local/bin/dodot-container-linux"
export DOTFILES_ROOT="${DOTFILES_ROOT:-/dotfiles}"
export TEST_HOME="/tmp/test-home"

# Helper function to create clean test environment
setup_test_home() {
  rm -rf "$TEST_HOME"
  mkdir -p "$TEST_HOME"
  export HOME="$TEST_HOME"
}

# Helper function to clean up after tests
cleanup_test_home() {
  rm -rf "$TEST_HOME"
}