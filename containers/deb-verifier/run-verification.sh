#!/bin/bash
set -euo pipefail

# Default to amd64 if not specified
ARCH="${1:-amd64}"
VERSION="${2:-0.0.6}"

# Validate architecture
if [[ "$ARCH" != "amd64" && "$ARCH" != "arm64" ]]; then
    echo "Error: Invalid architecture. Use 'amd64' or 'arm64'"
    exit 1
fi

# Construct the DEB URL
DEB_URL="https://github.com/arthur-debert/dodot/releases/download/v${VERSION}/dodot_${VERSION}_linux_${ARCH}.deb"

echo "Building deb-verifier Docker image..."
docker build -t deb-verifier .

echo -e "\nRunning verification for: $DEB_URL\n"
docker run --rm deb-verifier "$DEB_URL"