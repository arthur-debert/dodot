# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Build & Development
- `./scripts/build` - Builds the CLI binary with embedded version info to `./bin/my-awesome-cli`
- `./scripts/test` - Runs tests with race detection
- `./scripts/test-with-cov` - Runs tests with detailed coverage report and HTML output
- `./scripts/lint` - Runs golangci-lint (auto-installs if needed)
- `./scripts/pre-commit install` - Sets up git hooks for code quality

### Release & Distribution
- `./scripts/release-new --patch|--minor|--major` - Creates a new semantic version release
- `goreleaser release --clean` - Builds multi-platform releases (triggered by version tags)

### Running the CLI
- `./bin/my-awesome-cli --version` - Shows version info
- `./bin/my-awesome-cli -v/-vv/-vvv` - Increases logging verbosity

## Architecture Overview

### Command Structure
The CLI uses Cobra framework with a modular command structure:
- `cmd/my-awesome-cli/main.go` - Entry point with version variables set by ldflags
- `cmd/my-awesome-cli/root.go` - Root command setup with persistent flags and subcommands
- Commands include: `version`, `completion`, `man`

### Package Organization
- `pkg/logging/` - Centralized zerolog-based logging with verbosity control
- Additional packages should be added under `pkg/` for reusable functionality

### Build System
- GoReleaser handles multi-platform builds (Linux/macOS/Windows, amd64/arm64)
- Version info embedded via ldflags: `-X main.version={{.Version}} -X main.commit={{.Commit}}`
- Homebrew formula generation for macOS distribution
- Debian package generation for Linux distribution

### Key Dependencies
- `github.com/spf13/cobra` - CLI framework
- `github.com/rs/zerolog` - Structured logging
- Go 1.23+ required

### Testing Approach
- Use standard Go testing with race detection enabled
- Coverage reports uploaded to Codecov
- Run single tests: `go test -v -run TestName ./pkg/...`

### CI/CD Pipeline
- GitHub Actions workflow on push/PR: build, test, coverage
- Release workflow triggered by version tags (v*)
- Automated multi-platform builds and GitHub releases via GoReleaser