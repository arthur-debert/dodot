#!/bin/bash
set -e

echo "Building dodot development container..."

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Export user ID and group ID for the build
export USER_UID=$(id -u)
export USER_GID=$(id -g)

# Build the container
docker-compose build --build-arg USER_UID=$USER_UID --build-arg USER_GID=$USER_GID

echo "✅ Development container built successfully!"