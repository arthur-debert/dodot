#!/bin/bash
# Simple install script for testing install_script power-up

echo "Install script running with HOME=$HOME" >&2
mkdir -p "$HOME/.local/test"
echo "installed" > "$HOME/.local/test/marker.txt"
echo "Install script completed, marker file created" >&2