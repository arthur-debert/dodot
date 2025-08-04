#!/bin/bash
# Setup Phase - Build dodot binary inside container
#
# CRITICAL: This MUST be the first thing that runs. We cannot use a binary
# compiled on the host (macOS ARM64) inside the container (Linux). The binary
# must be compiled here with the container's architecture.
#
# We use the existing build script to ensure consistency.

set -euo pipefail

echo "=== Setup Phase: Building dodot ==="
echo

# Verify we have Go installed
if ! command -v go &> /dev/null; then
    echo "❌ ERROR: Go is not installed in the container"
    exit 1
fi

echo "Go version: $(go version)"
echo "Architecture: $(uname -m)"
echo

# Verify we have the source code mounted
if [ ! -f "/dodot/go.mod" ]; then
    echo "❌ ERROR: Source code not mounted at /dodot"
    echo "Make sure docker-run.sh mounts the repository"
    exit 1
fi

# Change to source directory
cd /dodot

# Configure git safe directory for GitHub Actions
# This is needed when running as root in a repo owned by another user
git config --global --add safe.directory /dodot

# Use the existing build script
# Skip tests during container setup for speed
echo "Building dodot using scripts/build..."
export SKIP_TESTS=true
if ./scripts/build; then
    echo "✅ Build completed successfully"
else
    echo "❌ Build failed"
    exit 1
fi

# The build script creates bin/dodot
if [ ! -f "bin/dodot" ]; then
    echo "❌ ERROR: Binary not found at expected location (bin/dodot)"
    exit 1
fi

# Install to system location with clear name
echo
echo "Installing binary to system location..."
sudo cp bin/dodot /usr/local/bin/dodot-container-linux
sudo chmod +x /usr/local/bin/dodot-container-linux

# Create a symlink for convenience
sudo ln -sf /usr/local/bin/dodot-container-linux /usr/local/bin/dodot

# Verify it works
echo
echo "Verifying binary..."
if /usr/local/bin/dodot-container-linux --version; then
    echo "✅ Binary verification successful"
    /usr/local/bin/dodot-container-linux --version
else
    # The build script already verified it works, so this is unexpected
    echo "❌ Binary verification failed"
    echo "This is unexpected as the build script already verified it"
    exit 1
fi

echo
echo "Setting up brew mocking..."
# Rename real brew to brew-full if it exists
if command -v brew &> /dev/null && [ ! -e /home/linuxbrew/.linuxbrew/bin/brew-full ]; then
    sudo mv /home/linuxbrew/.linuxbrew/bin/brew /home/linuxbrew/.linuxbrew/bin/brew-full
fi

# Install our mock brew
sudo cp /test-environment/scripts/mock-brew.sh /home/linuxbrew/.linuxbrew/bin/brew
sudo chmod +x /home/linuxbrew/.linuxbrew/bin/brew

echo "✅ Mock brew installed (real brew available as brew-full)"

echo
echo "Setup phase completed successfully!"