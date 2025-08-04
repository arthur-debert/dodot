#!/bin/zsh
# PowerUp Verification Functions
# 
# Standardized functions for verifying powerup deployments in tests.
# These functions reduce repetition and improve test maintainability.

# Verify symlink powerup deployment
# Usage: verify_symlink_deployed <pack> <filename> [target_dir]
# Example: verify_symlink_deployed "vim" ".vimrc"
# Example: verify_symlink_deployed "ssh" "config" ".ssh"
verify_symlink_deployed() {
  local pack=$1
  local filename=$2
  local target_dir=${3:-$HOME}
  local symlink_path="$target_dir/$filename"
  
  # Check symlink exists
  if [ ! -L "$symlink_path" ]; then
    echo "ERROR: Expected symlink at $symlink_path does not exist" >&2
    return 1
  fi
  
  # Verify it points somewhere (dodot creates direct symlinks to source)
  local link_target=$(readlink "$symlink_path")
  if [ -z "$link_target" ]; then
    echo "ERROR: Symlink $symlink_path has no target" >&2
    return 1
  fi
  
  # Verify the symlink target exists and is readable
  local resolved_target=$(readlink -f "$symlink_path" 2>/dev/null)
  if [ ! -e "$resolved_target" ]; then
    echo "ERROR: Symlink target does not exist: $link_target" >&2
    return 1
  fi
  
  # Verify content is accessible
  if [ ! -r "$symlink_path" ]; then
    echo "ERROR: Cannot read content through symlink $symlink_path" >&2
    return 1
  fi
  
  return 0
}

# Verify shell profile powerup deployment
# Usage: verify_shell_profile_deployed <pack> [source_file]
# Example: verify_shell_profile_deployed "bash" "aliases.sh"
verify_shell_profile_deployed() {
  local pack=$1
  local source_file=${2:-"aliases.sh"}
  local profile_dir="$HOME/.local/share/dodot/deployed/shell_profile"
  local deployed_file="$profile_dir/${pack}.sh"
  
  # Check directory exists
  if [ ! -d "$profile_dir" ]; then
    echo "ERROR: Shell profile directory $profile_dir does not exist" >&2
    return 1
  fi
  
  # Check symlink exists
  if [ ! -L "$deployed_file" ]; then
    echo "ERROR: Expected symlink at $deployed_file does not exist" >&2
    return 1
  fi
  
  # Verify symlink points to source
  local link_target=$(readlink "$deployed_file")
  if [[ "$link_target" != *"$pack/$source_file"* ]]; then
    echo "ERROR: Symlink $deployed_file points to $link_target, not $pack/$source_file" >&2
    return 1
  fi
  
  # Verify we can source it and check marker (if present)
  # Convention: <PACK>_PROFILE_LOADED=1
  local marker="${pack^^}_PROFILE_LOADED"
  if ! bash -c "source '$deployed_file' 2>/dev/null && [ \"\$$marker\" = '1' ]"; then
    # Marker check is optional for backward compatibility
    echo "WARNING: Shell profile marker $marker not found in $deployed_file" >&2
  fi
  
  return 0
}

# Verify shell add path powerup deployment
# Usage: verify_shell_add_path_deployed <pack> [bin_dir]
# Example: verify_shell_add_path_deployed "tools" "bin"
verify_shell_add_path_deployed() {
  local pack=$1
  local bin_dir=${2:-"bin"}
  local path_dir="$HOME/.local/share/dodot/deployed/path"
  local deployed_link="$path_dir/$pack"
  
  # Check directory exists
  if [ ! -d "$path_dir" ]; then
    echo "ERROR: Path directory $path_dir does not exist" >&2
    return 1
  fi
  
  # Check symlink exists
  if [ ! -L "$deployed_link" ]; then
    echo "ERROR: Expected symlink at $deployed_link does not exist" >&2
    return 1
  fi
  
  # Verify symlink points to bin directory
  local link_target=$(readlink "$deployed_link")
  if [[ "$link_target" != *"$pack/$bin_dir"* ]]; then
    echo "ERROR: Symlink $deployed_link points to $link_target, not $pack/$bin_dir" >&2
    return 1
  fi
  
  # Verify at least one executable exists
  if [ -z "$(find "$deployed_link" -type f -executable 2>/dev/null)" ]; then
    echo "ERROR: No executable files found in $deployed_link" >&2
    return 1
  fi
  
  return 0
}

# Verify install script powerup deployment
# Usage: verify_install_script_deployed <pack> [script_name] [marker_file]
# Example: verify_install_script_deployed "tools" "install.sh" "/tmp/tools-installed.marker"
verify_install_script_deployed() {
  local pack=$1
  local script_name=${2:-"install.sh"}
  local marker_file=$3
  local sentinel_dir="$HOME/.local/share/dodot/install"
  local sentinel_file="$sentinel_dir/$pack"
  
  # Check sentinel exists
  if [ ! -f "$sentinel_file" ]; then
    echo "ERROR: Sentinel file $sentinel_file does not exist" >&2
    return 1
  fi
  
  # Verify sentinel contains checksum (64 hex chars)
  if ! grep -qE "^[a-f0-9]{64}$" "$sentinel_file"; then
    echo "ERROR: Sentinel file $sentinel_file does not contain valid checksum" >&2
    return 1
  fi
  
  # If marker file specified, check it exists
  if [ -n "$marker_file" ]; then
    if [ ! -f "$marker_file" ]; then
      echo "ERROR: Expected marker file $marker_file does not exist" >&2
      return 1
    fi
  fi
  
  return 0
}

# Verify brewfile powerup deployment
# Usage: verify_brewfile_deployed <pack>
# Example: verify_brewfile_deployed "tools"
verify_brewfile_deployed() {
  local pack=$1
  local sentinel_file="$HOME/.local/share/dodot/brewfile/$pack"
  local brew_log="/tmp/brew-calls.log"
  
  # Check sentinel exists
  if [ ! -f "$sentinel_file" ]; then
    echo "ERROR: Sentinel file $sentinel_file does not exist" >&2
    return 1
  fi
  
  # Verify sentinel contains checksum (64 hex chars)
  if ! grep -qE "^[a-f0-9]{64}$" "$sentinel_file"; then
    echo "ERROR: Sentinel file $sentinel_file does not contain valid checksum" >&2
    return 1
  fi
  
  # Verify brew was called (for testing with mock)
  if [ -f "$brew_log" ]; then
    if ! grep -q "bundle --file.*$pack/Brewfile" "$brew_log"; then
      echo "ERROR: No brew bundle call found for $pack in $brew_log" >&2
      return 1
    fi
  fi
  
  return 0
}

# Composite function to verify multiple powerups for a pack
# Usage: verify_pack_deployed <pack> <powerup1> [<powerup2> ...]
# Example: verify_pack_deployed "bash" "symlink:.bashrc" "shell_profile:aliases.sh"
verify_pack_deployed() {
  local pack=$1
  shift
  
  local all_verified=0
  
  for powerup_spec in "$@"; do
    local powerup_type="${powerup_spec%%:*}"
    local powerup_arg="${powerup_spec#*:}"
    
    case "$powerup_type" in
      symlink)
        verify_symlink_deployed "$pack" "$powerup_arg" || all_verified=1
        ;;
      shell_profile)
        verify_shell_profile_deployed "$pack" "$powerup_arg" || all_verified=1
        ;;
      shell_add_path)
        verify_shell_add_path_deployed "$pack" "$powerup_arg" || all_verified=1
        ;;
      install_script)
        verify_install_script_deployed "$pack" "$powerup_arg" || all_verified=1
        ;;
      brewfile)
        verify_brewfile_deployed "$pack" || all_verified=1
        ;;
      *)
        echo "ERROR: Unknown powerup type: $powerup_type" >&2
        all_verified=1
        ;;
    esac
  done
  
  return $all_verified
}

# Helper function to add shell profile markers to test files
add_shell_profile_marker() {
  local pack=$1
  local file=$2
  echo "${pack^^}_PROFILE_LOADED=1" >> "$file"
}

# Verify idempotent deployment
verify_idempotent_deploy() {
  local pack=$1
  
  # Deploy twice
  "$DODOT" deploy "$pack" >/dev/null 2>&1
  local first_result=$?
  
  "$DODOT" deploy "$pack" >/dev/null 2>&1
  local second_result=$?
  
  # Both should succeed
  if [ $first_result -ne 0 ] || [ $second_result -ne 0 ]; then
    echo "ERROR: Idempotent deployment failed for pack $pack (first=$first_result, second=$second_result)" >&2
    return 1
  fi
  
  return 0
}