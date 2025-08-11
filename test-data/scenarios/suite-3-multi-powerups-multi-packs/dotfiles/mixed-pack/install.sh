#!/bin/bash
# Install script for mixed-pack

echo "Installing mixed-pack..." >&2

# Create install marker
mkdir -p "$HOME/.local/mixed-pack"
echo "mixed-pack-installed" > "$HOME/.local/mixed-pack/marker.txt"

echo "Mixed-pack installation complete" >&2