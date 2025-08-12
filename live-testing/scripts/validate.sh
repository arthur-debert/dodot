#!/bin/bash
set -e

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== Validating dodot development container ==="
echo ""

# Run a command in the container to test functionality
run_in_container() {
    docker compose -f "$SCRIPT_DIR/../containers/docker-compose.yml" run --rm dodot-dev /bin/bash -c "$1"
}

echo "1. Testing Go installation..."
run_in_container "go version"
echo "✓ Go installed"
echo ""

echo "2. Testing golangci-lint..."
run_in_container "golangci-lint --version"
echo "✓ golangci-lint installed"
echo ""

echo "3. Testing gotestsum..."
run_in_container "gotestsum --version"
echo "✓ gotestsum installed"
echo ""

echo "4. Testing goreleaser..."
run_in_container "goreleaser --version"
echo "✓ goreleaser installed"
echo ""

echo "5. Testing scripts/build..."
run_in_container "cd /workspace && ./scripts/build"
echo "✓ scripts/build works"
echo ""

echo "6. Testing scripts/lint..."
run_in_container "cd /workspace && ./scripts/lint"
echo "✓ scripts/lint works"
echo ""

echo "7. Testing scripts/pre-commit..."
run_in_container "cd /workspace && ./scripts/pre-commit"
echo "✓ scripts/pre-commit works"
echo ""

echo "8. Testing goreleaser build..."
run_in_container "cd /workspace && goreleaser build --snapshot --clean"
echo "✓ goreleaser build works"
echo ""

echo "9. Testing binary execution..."
run_in_container "cd /workspace && ./bin/dodot --version"
echo "✓ Binary runs successfully"
echo ""

echo "=== All validation tests passed! ==="