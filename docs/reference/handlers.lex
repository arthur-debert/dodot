Handlers Guide

    Handlers are operation generators in dodot that process matched files and convert them into simple operations (CreateDataLink, CreateUserLink, RunCommand). Each handler serves a specific purpose in managing your dotfiles.

1. Available Handlers

    dodot includes several built-in handlers, each with comprehensive documentation in their respective packages. For detailed end-to-end documentation of each handler, including examples, configuration, and best practices, see the doc.go files referenced below.

    1.1. SymlinkHandler (`symlink`)

        Creates symlinks from dotfiles to target locations. Use for configuration files and dotfiles.

        See `pkg/handlers/lib/symlink/doc.go`.

    1.2. InstallHandler (`install`)

        Executes shell scripts for one-time setup. Use for installing tools and initial setup.

        See `pkg/handlers/lib/install/doc.go`.

    1.3. HomebrewHandler (`homebrew`)

        Processes Brewfiles to manage Homebrew packages. Use for system packages and GUI apps (macOS).

        See `pkg/handlers/lib/homebrew/doc.go`.

    1.4. ShellProfileHandler (`shell`)

        Sources shell scripts into your environment. Use for aliases, functions, and shell customization.

        See `pkg/handlers/lib/shell/doc.go`.

    1.5. PathHandler (`path`)

        Adds directories to system PATH. Use for personal scripts and tools directories.

        See `pkg/handlers/lib/path/doc.go`.

2. Handler Categories

    Handlers are divided into two categories based on their operation types.

    2.1. Code Execution Handlers (provisioning)

        - Generate *RunCommand* operations with sentinels
        - Install Handler: runs setup scripts once
        - Homebrew Handler: installs packages once
        - Operations are tracked to prevent re-execution
        - Run by default with `dodot up`, skip with `--no-provision`

    2.2. Configuration Handlers (always run)

        - Generate *CreateDataLink* and *CreateUserLink* operations
        - Symlink Handler: creates configuration symlinks
        - Path Handler: manages PATH entries
        - Shell Handler: sources shell configuration
        - Operations are idempotent (safe to run multiple times)
        - Always run with `dodot up`

3. Quick Reference

    Handler summary:
        | Handler  | Category       | Operations                    | Purpose                  |
        | symlink  | Configuration  | CreateDataLink + CreateUserLink | Link configs to home   |
        | install  | Code Execution | RunCommand                    | Run setup scripts once   |
        | homebrew | Code Execution | RunCommand                    | Install packages once    |
        | shell    | Configuration  | CreateDataLink                | Source shell configs     |
        | path     | Configuration  | CreateDataLink                | Manage PATH entries      |
    :: table ::

4. Handler Documentation Structure

    Each handler's `doc.go` file contains:

    - Overview: what the handler does
    - When It Runs: Configuration vs Provisioning category
    - Standard Configuration: example rule configs
    - File Selection Process: how files are matched
    - Execution Strategy: how it works internally
    - Storage Locations: where files are stored
    - Effects on User Environment: what changes are made
    - Options: configuration options
    - Example End-to-End Flow: complete usage example
    - Error Handling: common errors and solutions
    - Best Practices: usage recommendations
    - Comparison: when to use vs other handlers

5. Creating Custom Handlers

    While dodot includes comprehensive built-in handlers, you can create custom ones:

    - Implement the `operations.Handler` interface (just 50-100 lines)
    - Add handler creation in `pkg/rules/integration.go`
    - Implement `ToOperations()` to convert matches to operations
    - Add rules to your configuration

    Handlers are simple data transformers: they just declare what operations they need, not how to perform them. See `pkg/operations/types.go` for the interface definition.

6. Best Practices

    - Read the `doc.go` files: each handler has extensive documentation
    - Use the right tool: each handler has a specific purpose
    - Check run modes: understand when handlers execute
    - Test with `--dry-run`: preview changes before applying
    - Organize by pack: group related configurations
    - Use standard patterns: follow naming conventions

7. Troubleshooting

    - Not running? Check rule patterns and priorities.
    - Errors? Each `doc.go` lists common error codes.
    - Wrong phase? Verify handler category (Configuration vs Code Execution).
    - Conflicts? Use `--force` flag when appropriate.

    For comprehensive details on any handler, read its `doc.go` file in the `pkg/handlers/<name>/` directory.
