#!/bin/bash
# Install script that depends on essential-tool

echo "Tools-consumer install script starting..." >&2

# Check if essential-tool symlink exists in HOME
if [ ! -L "$HOME/essential-tool" ] || [ ! -x "$HOME/essential-tool" ]; then
    echo "ERROR: essential-tool not found! Deploy tools-provider pack first." >&2
    exit 1
fi

# Use the tool via direct path
"$HOME/essential-tool" >&2

# Create marker to show successful install
mkdir -p "$HOME/.local/tools-consumer"
echo "installed-with-dependencies" > "$HOME/.local/tools-consumer/marker.txt"

echo "Tools-consumer installation complete" >&2