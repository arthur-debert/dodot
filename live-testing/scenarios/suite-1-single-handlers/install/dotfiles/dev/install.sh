#!/bin/bash
# Simple install script for testing install handler

echo "Install script running" >&2
echo "HOME=$HOME" >&2
echo "PWD=$PWD" >&2

# Create marker file in expected location
mkdir -p "$HOME/.local/test"
echo "installed" > "$HOME/.local/test/marker.txt"
echo "Install script completed, marker file created in $HOME/.local/test/marker.txt" >&2