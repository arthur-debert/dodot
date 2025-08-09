#!/bin/bash
# Test install script for tools pack

echo "Installing tools..."

# Create a marker file to verify the script ran
mkdir -p "$HOME/.local/dodot-test"
echo "tools installed at $(date)" > "$HOME/.local/dodot-test/tools-installed.txt"

echo "Tools installation complete!"