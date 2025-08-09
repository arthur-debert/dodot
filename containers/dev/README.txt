dodot Development Container
===========================

This directory contains the Docker-based development environment for dodot.

Quick Start
-----------
1. Build container: ./build.sh
2. Interactive shell: ./run.sh
3. Run command: ./run.sh <command>
4. Run tests: ./run-tests.sh
   - Quiet mode: ./run-tests.sh (default - shows only failures)
   - Verbose mode: ./run-tests.sh -v (shows all output)

Scripts
-------
- build.sh: Builds the dodot-dev Docker image
- run.sh: Runs commands or interactive shell in container
- run-tests.sh: Builds dodot and runs live system tests
- validate.sh: Validates container setup

The container includes:
- Go 1.24.5
- All development tools (golangci-lint, etc)
- Bats for integration testing
- Homebrew for testing

For more details, see docs/dev/containers.txxt