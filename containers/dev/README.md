# Dodot Development Containers

This directory contains various Docker configurations for dodot development and testing.

## Available Containers

### 1. Main Development Container (Dockerfile)
The primary development environment for dodot with Ubuntu 24.04 and all required tools.

### 2. Debian Package Test Container (Dockerfile.deb-test)
For testing Debian package builds and installations.

## Main Development Container Features

- Ubuntu 24.04 base image
- Go 1.24.5 with all required tools
- All dependencies needed for development:
  - golangci-lint for linting
  - gotestsum for enhanced test output
  - goreleaser for building releases
  - git, gh (GitHub CLI), direnv
  - npm/npx for any JavaScript tooling
  - Homebrew for package management
  - zsh with oh-my-zsh for a better shell experience
  - bat for better file viewing

## Usage

### Building the Container

```bash
./build.sh
```

### Running the Container

```bash
./run.sh
```

This will:
- Mount the repository root at /workspace
- Drop you into a zsh shell
- Preserve your command history between sessions
- Pass through your git configuration
- Cache Go dependencies for faster builds

### Available Commands

Once inside the container, you can run all project scripts:

- `./scripts/build` - Build the dodot binary
- `./scripts/test` - Run tests with race detection
- `./scripts/lint` - Run linting checks
- `./scripts/pre-commit` - Run pre-commit checks
- `goreleaser build --snapshot --clean` - Test goreleaser build

### Environment Variables

The container passes through:
- Git author/committer configuration (from host environment)
- GitHub tokens (GITHUB_TOKEN, GH_TOKEN)
- Homebrew tap token for releases

### Git Configuration

The container includes a pre-configured .gitconfig with:
- Pull strategy: rebase with fast-forward only
- Push: auto-setup remote, current branch
- Git LFS support
- VS Code as diff/merge tool
- Default branch: main

Note: Git user name and email are passed through from your host environment variables.

### Volumes

The container uses named volumes to persist:
- Go module cache (go-pkg)
- Go build cache (go-cache)
- Shell history (zsh-history)