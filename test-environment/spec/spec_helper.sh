#!/bin/zsh
# ShellSpec helper file for common test setup

# Set up test environment paths
# Use the container-built binary if available, otherwise use mock
if [ -x "/usr/local/bin/dodot-container-linux" ]; then
  export DODOT="/usr/local/bin/dodot-container-linux"
else
  # Use mock for simpler testing
  export DODOT="/test-environment/scripts/mock-dodot.sh"
  chmod +x "$DODOT"
fi
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
  # Fix permissions if they were changed during tests
  if [ -d "$TEST_HOME" ]; then
    chmod -R 755 "$TEST_HOME" 2>/dev/null || true
  fi
  rm -rf "$TEST_HOME" "$TEST_DOTFILES_ROOT"
}

# Comprehensive environment state verification
verify_clean_environment() {
  local errors=0
  
  # Only output to stderr if there are errors or DEBUG is set
  if [ -n "${DEBUG_TESTS:-}" ]; then
    echo "=== Verifying Clean Test Environment ===" >&2
  fi
  
  # 1. Verify HOME is set to TEST_HOME
  if [ "$HOME" != "$TEST_HOME" ]; then
    echo "ERROR: HOME is not set to TEST_HOME" >&2
    echo "  Expected: $TEST_HOME" >&2
    echo "  Actual: $HOME" >&2
    ((errors++))
  fi
  
  # 2. Verify DOTFILES_ROOT is set correctly
  if [ "$DOTFILES_ROOT" != "$TEST_DOTFILES_ROOT" ]; then
    echo "ERROR: DOTFILES_ROOT is not set to TEST_DOTFILES_ROOT" >&2
    echo "  Expected: $TEST_DOTFILES_ROOT" >&2
    echo "  Actual: $DOTFILES_ROOT" >&2
    ((errors++))
  fi
  
  # 3. Verify no dodot directories exist
  local dodot_dirs=(
    "$HOME/.local/share/dodot"
    "$HOME/.config/dodot"
    "$HOME/.cache/dodot"
    "$HOME/.local/state/dodot"
  )
  
  for dir in "${dodot_dirs[@]}"; do
    if [ -d "$dir" ]; then
      echo "ERROR: Dodot directory exists when it shouldn't: $dir" >&2
      ls -la "$dir" >&2
      ((errors++))
    fi
  done
  
  # 4. Verify no dodot-specific environment variables are set
  local dodot_env_vars=(
    "DOTFILES_HOME"
    "DODOT_DATA_DIR"
    "DODOT_CONFIG_DIR"
    "DODOT_CACHE_DIR"
    "DODOT_DEBUG"
  )
  
  for var in "${dodot_env_vars[@]}"; do
    # Use eval for zsh compatibility instead of ${!var}
    eval "value=\$$var"
    if [ -n "$value" ]; then
      echo "ERROR: $var is set when it shouldn't be: $value" >&2
      ((errors++))
    fi
  done
  
  # 5. Verify template directories were copied correctly
  if [ ! -f "$HOME/.zshrc" ]; then
    echo "ERROR: Template .zshrc not found in HOME" >&2
    ((errors++))
  fi
  
  if [ ! -d "$HOME/.config/existing-app" ]; then
    echo "ERROR: Template .config/existing-app not found" >&2
    ((errors++))
  fi
  
  if [ ! -d "$DOTFILES_ROOT/vim" ] || [ ! -d "$DOTFILES_ROOT/zsh" ]; then
    echo "ERROR: Template dotfiles directories not found" >&2
    ((errors++))
  fi
  
  # 6. Verify no unexpected files in HOME (excluding known container files)
  local unexpected_files=$(find "$HOME" -maxdepth 1 -type f -name ".*" | \
    grep -v -E "(.zshrc|.bashrc|.bash_logout|.profile|.zprofile)$" | wc -l)
  if [ "$unexpected_files" -gt 0 ]; then
    echo "WARNING: Unexpected dotfiles in HOME:" >&2
    find "$HOME" -maxdepth 1 -type f -name ".*" | \
      grep -v -E "(.zshrc|.bashrc|.bash_logout|.profile|.zprofile)$" >&2
  fi
  
  # 7. Verify no symlinks exist in HOME (should be none initially)
  local symlinks=$(find "$HOME" -maxdepth 1 -type l 2>/dev/null | wc -l)
  if [ "$symlinks" -gt 0 ]; then
    echo "ERROR: Unexpected symlinks in HOME:" >&2
    find "$HOME" -maxdepth 1 -type l -ls >&2
    ((errors++))
  fi
  
  # 8. Verify XDG directories exist but are empty (except existing-app)
  if [ ! -d "$HOME/.config" ] || [ ! -d "$HOME/.local/share" ] || [ ! -d "$HOME/.cache" ]; then
    echo "ERROR: XDG directories missing" >&2
    ((errors++))
  fi
  
  # Only output summary if there are errors or DEBUG is set
  if [ $errors -gt 0 ] || [ -n "${DEBUG_TESTS:-}" ]; then
    echo "=== Verification Complete: $errors errors found ===" >&2
  fi
  
  return $errors
}

# Debug function to dump complete environment state
dump_environment_state() {
  echo "=== ENVIRONMENT STATE DUMP ===" >&2
  echo "HOME=$HOME" >&2
  echo "DOTFILES_ROOT=$DOTFILES_ROOT" >&2
  echo "TEST_HOME=$TEST_HOME" >&2
  echo "TEST_DOTFILES_ROOT=$TEST_DOTFILES_ROOT" >&2
  
  echo -e "\n--- Dodot Environment Variables ---" >&2
  env | grep -E "^(DODOT_|DOTFILES_)" | sort >&2 || echo "None found" >&2
  
  echo -e "\n--- HOME Directory Structure ---" >&2
  find "$HOME" -type f -o -type l -o -type d | head -20 | sort >&2
  
  echo -e "\n--- Symlinks in HOME ---" >&2
  find "$HOME" -maxdepth 2 -type l -ls 2>/dev/null >&2 || echo "None found" >&2
  
  echo -e "\n--- Dodot Directories ---" >&2
  for dir in "$HOME/.local/share/dodot" "$HOME/.config/dodot" "$HOME/.cache/dodot" "$HOME/.local/state/dodot"; do
    if [ -d "$dir" ]; then
      echo "$dir exists:" >&2
      ls -la "$dir" >&2
    else
      echo "$dir does not exist" >&2
    fi
  done
  
  echo -e "\n--- DOTFILES_ROOT Structure ---" >&2
  find "$DOTFILES_ROOT" -name "*.toml" -o -name "pack.dodot.toml" | sort >&2
  
  echo "=== END ENVIRONMENT STATE DUMP ===" >&2
}

# Enhanced reset function that verifies clean state
reset_test_environment() {
  # Fix permissions if they were changed during tests
  if [ -d "$TEST_HOME" ]; then
    chmod -R 755 "$TEST_HOME" 2>/dev/null || true
  fi
  
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
  
  # Clear any dodot-specific directories (belt and suspenders)
  rm -rf "$HOME/.local/share/dodot" "$HOME/.cache/dodot" "$HOME/.config/dodot" "$HOME/.local/state/dodot"
  
  # Unset any dodot-specific environment variables
  unset DOTFILES_HOME DODOT_DATA_DIR DODOT_CONFIG_DIR DODOT_CACHE_DIR DODOT_DEBUG
  
  # Verify we have a clean environment
  if ! verify_clean_environment; then
    echo "FATAL: Environment verification failed after reset!" >&2
    return 1
  fi
}