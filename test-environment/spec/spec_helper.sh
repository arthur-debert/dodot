#!/bin/zsh
# ShellSpec helper file for common test setup

# Set up test environment paths
export DODOT="/usr/local/bin/dodot-container-linux"
export TEST_HOME="/tmp/test-home"
export TEST_DOTFILES_ROOT="/tmp/test-dotfiles"
export HOME_TEMPLATE="/test-environment/home-template"
export DOTFILES_TEMPLATE="/test-environment/dotfiles-root-template"

# Reset test environment to clean state using templates
reset_test_environment() {
  # Clean up any existing test directories
  rm -rf "$TEST_HOME" "$TEST_DOTFILES_ROOT"
  
  # Create fresh copies from templates
  cp -r "$HOME_TEMPLATE" "$TEST_HOME"
  cp -r "$DOTFILES_TEMPLATE" "$TEST_DOTFILES_ROOT"
  
  # Set environment variables for dodot
  export HOME="$TEST_HOME"
  export DOTFILES_ROOT="$TEST_DOTFILES_ROOT"
  
  # Create XDG directories if they don't exist in template
  mkdir -p "$HOME/.config" "$HOME/.local/share" "$HOME/.local/state" "$HOME/.cache"
  
  # Clear any dodot-specific directories
  rm -rf "$HOME/.local/share/dodot" "$HOME/.cache/dodot"
}

# Helper to verify file is a symlink pointing to expected target
verify_symlink() {
  local link_path="$1"
  local expected_target="$2"
  
  if [ -L "$link_path" ]; then
    local actual_target=$(readlink "$link_path")
    if [ "$actual_target" = "$expected_target" ]; then
      return 0
    else
      echo "Symlink $link_path points to $actual_target, not $expected_target"
      return 1
    fi
  else
    echo "$link_path is not a symlink"
    return 1
  fi
}

# Helper to check if file exists and is regular file
verify_regular_file() {
  local file_path="$1"
  
  if [ -f "$file_path" ] && [ ! -L "$file_path" ]; then
    return 0
  else
    echo "$file_path is not a regular file"
    return 1
  fi
}

# Clean up function
cleanup_test_environment() {
  rm -rf "$TEST_HOME" "$TEST_DOTFILES_ROOT"
}