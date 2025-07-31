# DEB Package Verifier

This Docker-based tool verifies that dodot's .deb packages install and function correctly on Ubuntu.

## Usage

### Quick verification with convenience script:
```bash
# Test amd64 package (default)
./run-verification.sh

# Test arm64 package
./run-verification.sh arm64

# Test specific version
./run-verification.sh amd64 0.0.7
```

### Manual Docker commands:
```bash
# Build the image
docker build -t deb-verifier .

# Run verification
docker run --rm deb-verifier https://github.com/arthur-debert/dodot/releases/download/v0.0.6/dodot_0.0.6_linux_amd64.deb
```

## What it tests

1. **Download**: Can the .deb file be downloaded from GitHub releases
2. **Package Info**: Shows package metadata
3. **Installation**: Installs the package using dpkg
4. **Binary**: Verifies dodot binary is in PATH
5. **Version**: Runs `dodot version` command
6. **Help**: Runs `dodot --help` command
7. **Man Page**: Checks if man page was installed
8. **Completions**: Checks bash, zsh, and fish completions
9. **File List**: Shows all installed files

## Output

- ✓ Green checkmarks indicate successful tests
- ✗ Red X marks indicate failures
- ⚠ Yellow warnings for optional components