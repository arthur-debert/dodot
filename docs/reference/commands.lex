dodot Commands Reference

    This document provides a comprehensive reference for all dodot commands, their functionality, and current implementation status.

1. Command Overview

    All commands follow a consistent pattern:

    - Use the core pipeline system for processing
    - Integrate with the datastore for state management
    - Provide proper error handling and logging
    - Support filesystem injection for testing

2. Commands

    2.1. addignore

        Creates a `.dodotignore` file in a specified pack to mark it as ignored. The command is idempotent and handles cases where the file already exists gracefully.

        Status:
            Working.

        Usage:
            `dodot addignore <pack-name>`

    2.2. adopt

        Moves existing files from the system into a pack and creates symlinks back to their original locations. Supports force mode to overwrite existing destinations and includes rollback capabilities if symlink creation fails.

        Status:
            Working.

        Usage:
            `dodot adopt <pack-name> <file-paths...> [--force]`

    2.3. fill

        Adds template/placeholder files to an existing pack based on configured handlers. Creates example configuration files, scripts, and directories that users can customize for their specific needs.

        Status:
            Working.

        Usage:
            `dodot fill <pack-name>`

    2.4. genconfig

        Generates default configuration content (commented out) and can write it to packs or current directory. Uses the same configuration template system as the init command for consistency.

        Status:
            Working.

        Usage:
            `dodot genconfig [--write] [pack-names...]`

    2.5. init (initialize)

        Creates a new pack directory with initial structure including configuration file, README, and template files. Leverages the fill command internally to ensure consistent template generation across commands.

        Status:
            Working.

        Usage:
            `dodot init <pack-name>`

    2.6. list

        Discovers and lists all available packs in the dotfiles root directory. Simple command that uses the core pack discovery infrastructure.

        Status:
            Working.

        Usage:
            `dodot list`

    2.7. down

        The primary pack removal command. Completely removes pack deployments including all symlinks, shell integrations, PATH entries, and handler state from the data directory. This is a complete removal, no state is saved for restoration. Files in your dotfiles repository are never touched.

        Status:
            Working.

        Usage:
            `dodot down [pack-names...]`

    2.8. up

        The primary pack deployment command. Handles all aspects of pack deployment including creating symlinks for configuration files, setting up shell integrations and PATH entries, and running installation scripts and package managers. By default, provisioning handlers only run once per pack.

        Options:
            - `--no-provision`: skip provisioning handlers (only link files)
            - `--provision-rerun`: force re-run provisioning even if already done

        Status:
            Working.

        Usage:
            `dodot up [pack-names...] [--no-provision | --provision-rerun]`

    2.9. status

        Shows deployment state of packs including special files, handler matches, and current deployment status. Uses the datastore to check actual deployment state and provides detailed file-level status information.

        Status:
            Working.

        Usage:
            `dodot status [pack-names...]`

3. Command Relationships

    3.1. Activation Flow

        `init` then `fill` then `up`. The up command handles both configuration and provisioning.

    3.2. Deactivation Flow

        `down` performs complete removal. All handler state is cleared.

    3.3. Handler Types

        Configuration Handlers:
            symlink, shell, path (always run).

        Code Execution Handlers:
            homebrew, install (run once by default, controllable via options).

4. Implementation Notes

    All commands share common infrastructure:

    - Pack discovery and selection via `pkg/packs/discovery`
    - Command execution through `pkg/packs/orchestration`
    - Handler execution through the rules system
    - State persistence using the datastore
    - Consistent error handling with error codes
    - Structured logging with verbosity levels

    The codebase maintains good separation between command logic (in `pkg/packs/commands`) and core functionality (in `pkg/packs`, `pkg/handlers`, `pkg/rules`).
