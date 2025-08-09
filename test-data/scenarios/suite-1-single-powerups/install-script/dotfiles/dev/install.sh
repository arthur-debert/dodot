#!/bin/bash
# Simple install script for testing install_script power-up

echo "Install script running" >&2
echo "HOME=$HOME" >&2
echo "PWD=$PWD" >&2

# Create marker file in current directory
echo "installed" > install-marker.txt
echo "Install script completed, marker file created in $PWD/install-marker.txt" >&2