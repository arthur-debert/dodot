#!/bin/bash
# Build dodot Docker images in the correct order
#
# Usage: ./build-images.sh [base|dev-env|all]
#
# Options:
#   base     - Build only the base image
#   dev-env  - Build only the dev-env image (requires base to exist)
#   all      - Build both images in order (default)
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Navigate to the containers directory
cd "$SCRIPT_DIR/../containers"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Function to build an image
build_image() {
    local dockerfile=$1
    local tag=$2
    local build_args=${3:-}
    
    print_info "Building $tag from $dockerfile..."
    
    if [ -n "$build_args" ]; then
        docker compose build --build-arg "$build_args" "${tag}"
    else
        docker compose build "${tag}"
    fi
    
    if [ $? -eq 0 ]; then
        print_info "Successfully built $tag"
    else
        print_error "Failed to build $tag"
        exit 1
    fi
}

# Function to check if an image exists
image_exists() {
    local image=$1
    docker images --format "{{.Repository}}:{{.Tag}}" | grep -q "^${image}$"
}

# Parse command line argument
TARGET=${1:-all}

case "$TARGET" in
    base)
        build_image "Dockerfile.base" "dodot-base"
        ;;
    
    dev-env)
        # Check if base image exists
        if ! image_exists "dodot-base:latest"; then
            print_warning "Base image not found, building it first..."
            build_image "Dockerfile.base" "dodot-base"
        fi
        build_image "Dockerfile.dev-env" "dodot-dev-env" "BASE_IMAGE=dodot-base:latest"
        ;;
    
    all)
        # Build in order
        build_image "Dockerfile.base" "dodot-base"
        build_image "Dockerfile.dev-env" "dodot-dev-env" "BASE_IMAGE=dodot-base:latest"
        print_info "All images built successfully!"
        ;;
    
    *)
        print_error "Unknown target: $TARGET"
        echo "Usage: $0 [base|dev-env|all]"
        exit 1
        ;;
esac

# Show image sizes
echo ""
print_info "Docker image sizes:"
docker images --format "table {{.Repository}}\t{{.Tag}}\t{{.Size}}" | grep -E "(REPOSITORY|dodot-)"