#!/usr/bin/env bash
# Minimal test runner - just safety checks and pass everything to Bats

set -e

# Safety check: ONLY run tests inside Docker container
if [ ! -f "/.dockerenv" ] && [ ! -f "/run/.containerenv" ]; then
    echo "ERROR: TESTS MUST RUN INSIDE DOCKER CONTAINER"
    echo "Running tests outside container could damage your system!"
    echo ""
    echo "Please use: ./scripts/run-live-tests"
    exit 1
fi

# Set test environment marker
export DODOT_TEST_CONTAINER=1

# Clean up any stale template test files
if [ -f "/workspace/live-testing/scripts/cleanup-template-tests.sh" ]; then
    /workspace/live-testing/scripts/cleanup-template-tests.sh >&2
fi

# Prevent Go from auto-downloading toolchains
export GOTOOLCHAIN=local

# Ensure Go build cache directory exists with correct permissions
sudo mkdir -p "$HOME/.cache/go-build" 2>/dev/null || true
sudo chown -R $(id -u):$(id -g) "$HOME/.cache" 2>/dev/null || true

# Detect OS and architecture inside container
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
    x86_64) ARCH="amd64" ;;
    aarch64|arm64) ARCH="arm64" ;;
    *) echo "ERROR: Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac
BINARY_NAME="dodot.${OS}-${ARCH}"

# Ensure dodot is built for this platform
if [ ! -x "/workspace/bin/${BINARY_NAME}" ]; then
    echo "Building dodot for ${OS}/${ARCH}..." >&2
    # Force the correct GOOS and GOARCH for the container
    GOOS=${OS} GOARCH=${ARCH} /workspace/scripts/build >&2 || {
        echo "ERROR: Failed to build dodot" >&2
        exit 1
    }
fi

# Verify the binary actually works
if ! /workspace/bin/dodot --version >/dev/null 2>&1; then
    echo "ERROR: dodot binary exists but doesn't work" >&2
    echo "Expected binary: /workspace/bin/${BINARY_NAME}" >&2
    ls -la /workspace/bin/ >&2
    exit 1
fi
export PATH="/workspace/bin:$PATH"

# Check if any of the args are test files/directories
has_test_files=false
for arg in "$@"; do
    if [[ "$arg" == *.bats ]] || [[ -d "$arg" ]] || [[ -f "$arg" ]]; then
        has_test_files=true
        break
    fi
done

# If no test files in args, find and append all test files
if [ "$has_test_files" = false ]; then
    # Find all test files
    test_files=()
    while IFS= read -r -d '' file; do
        test_files+=("$file")
    done < <(find /workspace/live-testing/scenarios -name "*.bats" -type f -print0 | sort -z)
    
    if [ ${#test_files[@]} -eq 0 ]; then
        echo "ERROR: No test files found" >&2
        exit 1
    fi
    
    # Append test files to existing args (like --formatter junit)
    set -- "$@" "${test_files[@]}"
fi

# Just run bats with whatever args we have
exec bats "$@"