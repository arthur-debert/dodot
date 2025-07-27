# Template Usage Instructions

This document explains how to use this Golang CLI template to create your own CLI application.

## Step-by-Step Setup

### 1. Copy Template Files

Copy the entire `golang-cli/` directory to your new project location:

```bash
cp -r golang-cli/ /path/to/your/new-project/
cd /path/to/your/new-project/
```

### 2. Replace Placeholders

Use find and replace to update all placeholder values. Here's a script to help:

```bash
#!/bin/bash

# Define your values
PKG_NAME="your-cli-name"
GITHUB_USER="your-github-username"  
PKG_DESCRIPTION="Description of your CLI tool"
AUTHOR_NAME="Your Name"
AUTHOR_EMAIL="your.email@example.com"

# Files to update
find . -type f \( -name "*.go" -o -name "*.yml" -o -name "*.yaml" -o -name "*.md" -o -name "go.mod" \) -exec sed -i '' \
  -e "s/PKG_NAME_PLACEHOLDER/$PKG_NAME/g" \
  -e "s/GITHUB_USER_PLACEHOLDER/$GITHUB_USER/g" \
  -e "s/PKG_DESCRIPTION_PLACEHOLDER/$PKG_DESCRIPTION/g" \
  -e "s/AUTHOR_NAME_PLACEHOLDER/$AUTHOR_NAME/g" \
  -e "s/AUTHOR_EMAIL_PLACEHOLDER/$AUTHOR_EMAIL/g" \
  {} +

# Rename directories
mv "cmd/PKG_NAME_PLACEHOLDER" "cmd/$PKG_NAME"
```

### 3. Initialize Git Repository

```bash
git init
git add .
git commit -m "Initial commit from golang-cli template"
```

### 4. Set Up Go Module

```bash
go mod tidy
```

### 5. Test the Setup

```bash
# Test build
./scripts/build

# Test CLI
./bin/your-cli-name --help
./bin/your-cli-name version

# Run tests
./scripts/test
```

### 6. GitHub Repository Setup

1. Create a new repository on GitHub
2. Add remote and push:

   ```bash
   git remote add origin https://github.com/your-username/your-cli-name.git
   git branch -M main
   git push -u origin main
   ```

3. Set up repository secrets:
   - `HOMEBREW_TAP_TOKEN`: GitHub token with access to homebrew-tools repo
   - `CODECOV_TOKEN`: Token for coverage reporting

### 7. Development Workflow

1. Install pre-commit hooks:

   ```bash
   ./scripts/pre-commit install
   ```

2. Start developing your CLI:
   - Add commands to `cmd/your-cli-name/`
   - Add packages to `pkg/`
   - Write tests

3. Create your first release:

   ```bash
   ./scripts/release-new --patch
   ```

## Template Features Explained

### Debug Mode for Homebrew

The template includes support for debug mode when releasing to Homebrew:

- **Production**: Formula goes to `Formula/` directory in homebrew-tools repo
- **Debug**: Formula goes to `debug/` directory for testing

To enable debug mode, set the `DEBUG` environment variable:

```bash
export DEBUG=1
# Now releases will use debug directory
```

### Directory Structure After Setup

```
your-cli-name/
├── .github/
│   └── workflows/
│       ├── test.yml          # CI testing
│       └── release.yml       # Release automation  
├── cmd/
│   └── your-cli-name/        # Main CLI application
│       ├── main.go          # Entry point
│       └── root.go          # Cobra root command
├── pkg/                     # Reusable packages
├── scripts/                 # Development scripts
├── .goreleaser.yml         # Release configuration
├── go.mod                  # Go module definition
└── README.md               # Your project documentation
```

### Customization Points

#### Add New Commands

Add new commands to your CLI by creating new files in `cmd/your-cli-name/`:

```go
// cmd/your-cli-name/serve.go
package main

import (
    "fmt"
    "github.com/spf13/cobra"
)

var serveCmd = &cobra.Command{
    Use:   "serve",
    Short: "Start the server",
    Run: func(cmd *cobra.Command, args []string) {
        fmt.Println("Starting server...")
    },
}

func init() {
    rootCmd.AddCommand(serveCmd)
}
```

#### Add Business Logic

Create packages in `pkg/` for your business logic:

```go
// pkg/server/server.go
package server

type Server struct {
    Port int
}

func NewServer(port int) *Server {
    return &Server{Port: port}
}

func (s *Server) Start() error {
    // Server implementation
    return nil
}
```

#### Customize Build Process

Modify `scripts/build` to add custom build steps:

```bash
# Add custom steps before or after the main build
echo "Running custom pre-build steps..."
# Your custom commands here

# Main build happens here...

echo "Running custom post-build steps..."  
# Your custom commands here
```

## Advanced Configuration

### GoReleaser Customization

Edit `.goreleaser.yml` to:

- Add more build targets
- Configure different archive formats
- Add Docker images
- Set up Scoop packages (Windows)
- Configure Snapcraft (Linux)

### GitHub Actions Enhancement

Enhance workflows by:

- Adding more test matrices (different Go versions)
- Adding integration tests
- Setting up deployment to other platforms
- Adding security scanning

### Development Scripts

The template includes these development scripts:

- **`build`**: Comprehensive build with testing
- **`test`**: Test runner with coverage
- **`lint`**: Code linting and formatting
- **`pre-commit`**: Git hook management
- **`release-new`**: Version management and tagging
- **`cloc-go`**: Code statistics
- **`test-with-coverage`**: Detailed coverage analysis

All scripts are designed to be:

- **Self-contained**: Install dependencies as needed
- **Cross-platform**: Work on macOS, Linux, and Windows
- **Configurable**: Support environment variables and flags
- **Robust**: Include error handling and cleanup

## Troubleshooting

### Common Issues

1. **Scripts not executable**: Run `chmod +x scripts/*`
2. **Go module issues**: Run `go mod tidy`
3. **Build failures**: Check Go version (requires 1.23+)
4. **Missing dependencies**: Scripts will auto-install most tools

### Getting Help

- Check script help: `./scripts/script-name --help`
- Review GitHub Actions logs for CI issues
- Ensure all placeholders are replaced
- Verify GitHub secrets are set correctly

## Next Steps

1. **Customize** the CLI for your specific use case
2. **Add tests** for your functionality
3. **Write documentation** for your CLI commands
4. **Set up** monitoring and error tracking
5. **Create** a user-friendly installation guide
6. **Consider** adding shell completions
7. **Plan** your release strategy and versioning scheme
