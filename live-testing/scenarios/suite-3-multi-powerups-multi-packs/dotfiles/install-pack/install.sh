#!/bin/bash
# Install script for install-pack

echo "Installing install-pack..." >&2

# Create install marker
mkdir -p "$HOME/.local/install-pack"
echo "install-pack-installed" > "$HOME/.local/install-pack/marker.txt"

echo "Install-pack installation complete" >&2