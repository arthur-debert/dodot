#!/usr/bin/env fish
# dodot-init.fish - Shell integration script for dodot (Fish shell)
# This script is sourced by the user's shell configuration to integrate dodot

# Determine DODOT_DATA_DIR
if test -z "$DODOT_DATA_DIR"
    if test -n "$XDG_DATA_HOME"
        set -gx DODOT_DATA_DIR "$XDG_DATA_HOME/dodot"
    else
        set -gx DODOT_DATA_DIR "$HOME/.local/share/dodot"
    end
end

# Ensure the data directory exists
if not test -d "$DODOT_DATA_DIR"
    return 0
end

# Define the deployed directory
set -g DODOT_DEPLOYED_DIR "$DODOT_DATA_DIR/deployed"

# 1. Source all shell profile scripts (aliases, environment variables, etc.)
if test -d "$DODOT_DEPLOYED_DIR/shell_profile"
    for script in "$DODOT_DEPLOYED_DIR/shell_profile"/*.sh
        if test -f "$script" -a -r "$script"
            # Check if the symlink target exists
            if test -e "$script"
                # Try to source bash scripts using bass if available
                if type -q bass
                    bass source "$script" 2>/dev/null
                end
            end
        end
    end
end

# 2. Add all directories to PATH
if test -d "$DODOT_DEPLOYED_DIR/path"
    for dir in "$DODOT_DEPLOYED_DIR/path"/*
        if test -d "$dir" -a -r "$dir"
            # Check if the symlink target exists
            if test -e "$dir"
                # Prepend to PATH to give precedence to dodot-managed bins
                set -gx PATH "$dir" $PATH
            end
        end
    end
end

# 3. Source additional shell files (fish-specific)
if test -d "$DODOT_DEPLOYED_DIR/shell_source"
    # Source fish files
    for script in "$DODOT_DEPLOYED_DIR/shell_source"/*.fish
        if test -f "$script" -a -r "$script"
            if test -e "$script"
                source "$script"
            end
        end
    end
end

# Export DODOT_DATA_DIR for potential use by dodot commands
set -gx DODOT_DATA_DIR $DODOT_DATA_DIR