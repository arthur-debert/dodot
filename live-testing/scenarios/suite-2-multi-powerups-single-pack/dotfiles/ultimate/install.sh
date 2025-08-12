#!/bin/bash
# Ultimate pack install script

echo "Installing ultimate pack..." >&2

# Create installation marker
mkdir -p "$HOME/.local/ultimate"
echo "ultimate-installed" > "$HOME/.local/ultimate/marker.txt"

echo "Ultimate pack installation complete" >&2