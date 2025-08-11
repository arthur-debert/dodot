dodot Development Container
===========================

This directory contains the Docker-based development environment for dodot which also serves as the container for 
live-systems testing.

Quick Start
-----------
1. Build container: ./build.sh
2. Interactive shell: ./run.sh
3. Run command: ./run.sh <command>
4. Run tests: ./run-tests.sh
   - Use -v flag to see build output (quiet by default)

Inside the container, all scripts/* commands are available including the goreleaser building.

The repository is mounted at /workspace, so you can edit files on your host machine and see changes reflected in the container.

When the container runs, it will build a dododt binary under bin/, which will already be in your PATH. Unless the container and the host are using the same architecture, the build will not be usable on the host machine. 



