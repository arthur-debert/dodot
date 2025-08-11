#!/bin/bash
# Install script for dev-tools pack

echo "Installing dev tools..." >&2

# Create a marker to verify the script ran
mkdir -p "$HOME/.local/dev-tools"
echo "dev-tools-installed" > "$HOME/.local/dev-tools/install-marker.txt"

echo "Dev tools installation complete" >&2