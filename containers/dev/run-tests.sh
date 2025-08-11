#!/bin/bash
# Run dodot live system tests - passes all arguments to Bats
#
# Usage: ./run-tests.sh [BATS_OPTIONS] [TEST_FILES...]
# 
# All arguments are passed directly to Bats inside the container.
# Output goes to stdout (format depends on --formatter option)
# For human-friendly output, use run-tests-pretty.sh

set -e

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Just run the runner.sh in docker, passing all args
exec "$SCRIPT_DIR/run.sh" /workspace/test-data/runner.sh "$@"