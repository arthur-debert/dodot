dodot Developer Guidelines
========================

This document establishes the development standards and practices for the dodot project.
All contributors must read and follow these guidelines.


Quick Reference
---------------

This section provides a condensed overview of the essential guidelines.

### Getting Started

1.  Source `.envrc` for environment variables
2.  Install pre-commit hooks: `scripts/pre-commit install`
3.  Run `scripts/test` to verify setup
4.  Read `docs/dev/txxt-primer.txxt` for documentation format

### Essential Commands

*   `scripts/build` - Build CLI binary with embedded version
*   `scripts/test` - Run tests with race detection
*   `scripts/test-with-cov` - Run tests with coverage report
*   `scripts/lint` - Run `golangci-lint` (auto-installs if needed)
*   `scripts/pre-commit install` - Set up mandatory git hooks

### Core Standards

*   **Pre-commit Hooks (MANDATORY)**: No commits allowed to skip `scripts/pre-commit`.
*   **Error Handling**: All errors MUST have codes and messages (`pkg/errors`). Test codes, not strings.
*   **Logging**: Logging is MANDATORY. Use structured `zerolog` (`pkg/logging`).
*   **Filesystem**: NO direct filesystem operations. ALL operations go through `synthfs`.
*   **Documentation**: ALL docs MUST use `txxt` format. Comment WHY, not WHAT.


Detailed Guidelines
-------------------

### 1. Documentation Standards

#### 1.1. Format Requirements

*   ALL project documentation MUST use `txxt` format, never Markdown.
*   Even `.txt` files should be `txxt` formatted (for AI assistant compatibility).
*   Reference `docs/dev/txxt-primer.txxt` for formatting guidelines.
*   Use consistent indentation (4 spaces) throughout.

#### 1.2. Documentation Locations

*   Design documents: `docs/design/`
*   Developer guides: `docs/dev/`
*   User documentation: `docs/`
*   API documentation: Inline with code using Go doc comments.

### 2. Code Quality Standards

#### 2.1. Pre-commit Hooks

*   NO commits are allowed to skip `scripts/pre-commit`.
*   Pre-commit automatically runs:
    *   `scripts/lint` (`golangci-lint` with project config)
    *   `scripts/test` (`go test` with race detection)
*   Fix all issues before committing.
*   Install hooks: `scripts/pre-commit install`

#### 2.2. Logging Requirements

*   Logging is MANDATORY for all new code.
*   Follow guidelines in `pkg/logging/logging.go`.
*   Use structured logging with `zerolog`.
*   Default level: `WARN`.
*   Include context fields for traceability.
*   Example:
    ```go
    logger.Debug().
        Str("pack", packName).
        Str("trigger", triggerName).
        Msg("processing trigger match")
    ```

#### 2.3. Error Handling

*   ALL errors MUST have both code and message.
*   Use the `DodotError` type from `pkg/errors`.
*   Error codes enable:
    *   Stable testing (test codes, not strings)
    *   Future internationalization
    *   Programmatic error handling
*   Test error codes explicitly:
    ```go
    assert.Equal(t, errors.TRIGGER_NOT_FOUND, err.Code)
    ```
*   Never test error message strings.

#### 2.4. Type Safety

*   Write type-safe code throughout.
*   Avoid `interface{}` unless absolutely necessary.
*   No magic strings - use constants.
*   Define types for domain concepts.
*   Use Go's type system to prevent errors.

### 3. Architecture Guidelines

#### 3.1. CLI Layer

*   CLI MUST be Cobra-based and thin.
*   Business logic belongs in `pkg/`, not `cmd/`.
*   CLI only handles:
    *   Argument parsing
    *   Flag validation
    *   Calling business logic
    *   Formatting output
*   No direct file system operations in CLI.

#### 3.2. File System Safety

*   NO file system changes in `dodot` code directly.
*   ALL operations go through `synthfs`.
*   Only E2E tests should execute real FS operations.
*   Benefits:
    *   Code is pure and testable
    *   Operations are transactional
    *   Dry-run capability built-in
    *   Rollback support

#### 3.3. User-Facing Strings

*   Keep all user-facing strings as module-level constants.
*   Place at the beginning of each file.
*   Enables easy review and future i18n.
*   Example:
    ```go
    const (
        msgPackNotFound = "pack not found: %s"
        msgInvalidConfig = "invalid configuration in %s"
    )
    ```

### 4. Development Practices

#### 4.1. File Organization

*   Do NOT litter the repo with test files or status files.
*   Use `PROJECT_ROOT/tmp/` for all temporary files.
*   Add `tmp/` to `.gitignore`.
*   Clean up after tests.

#### 4.2. Environment Variables

*   Use variables from `.envrc` to avoid hardcoding.
*   Key variables:
    *   `PROJECT_ROOT`: Repository root
    *   `DOTFILES_TEST_ROOT`: Test fixtures location
    *   `XDG_CONFIG_HOME`: Config directory
*   Source `.envrc` before development.

#### 4.3. Test Helpers

*   Leverage common test setup helpers extensively.
*   Common tasks have helpers:
    *   Setting up test dotfiles directories
    *   Creating mock pack structures
    *   Initializing test registries
    *   Capturing log output
*   See `pkg/testutil` for available helpers.
*   Add new helpers for repeated patterns.

### 5. Commit and PR Guidelines

#### 5.1. Commit Messages

*   When working on releases/milestones:
    *   Format: `"Release X.Y.Z: description"`
    *   Example: `"Release 3.4.2: implement FileNameTrigger with glob support"`
*   When fixing GitHub issues:
    *   Include `"fixes #XX"` to auto-close issues.
    *   Example: `"fixes #42: correct PATH handling in bin powerup"`

#### 5.2. Pull Request Process

*   Create PR as soon as first commit is done.
*   Enables early feedback and visibility.
*   Keep PRs focused on a single feature/fix.
*   Update PR description as work progresses.

#### 5.3. PR Completion

*   Use `gh` CLI to verify before merging:
    *   `gh pr checks`
    *   `gh pr view --web`
*   Ensure:
    *   All CI checks pass
    *   No merge conflicts
    *   Code review approved
    *   Branch is clean to merge

### 6. Code Comments

#### 6.1. Purpose of Comments

*   Explain WHY, not WHAT.
*   Document tricky bits or gotchas.
*   Record design decisions.
*   Explain use cases.
*   Reference relevant design docs.

### 7. Testing Guidelines

#### 7.1. Test Organization

*   Unit tests: Next to code (`*_test.go`)
*   Integration tests: `tests/integration/`
*   E2E tests: `tests/e2e/`
*   Minimize E2E tests (slow, flaky).

#### 7.2. Table-Driven Tests

*   Prefer table-driven tests for comprehensive coverage.
*   Makes adding test cases easy.
*   Clear test names for each case.

#### 7.3. Test Isolation

*   Each test must be independent.
*   Use `t.TempDir()` for test directories.
*   Don't rely on test execution order.
*   Clean up resources in `defer` statements.

### 8. Development Workflow

#### 8.1. Starting Work

1.  Source `.envrc`
2.  Ensure pre-commit hooks are installed
3.  Pull the latest `main` branch
4.  Create a feature branch

#### 8.2. During Development

1.  Write tests first (TDD encouraged).
2.  Implement with logging.
3.  Run `scripts/test` frequently.
4.  Run `scripts/lint` before committing.
5.  Keep commits focused and atomic.

#### 8.3. Completing Work

1.  Ensure all tests pass.
2.  Update relevant documentation.
3.  Create/update PR.
4.  Verify CI passes.
5.  Request review.

### 9. Performance Considerations

*   Profile before optimizing.
*   Batch operations where possible.
*   Use `sync.Pool` for expensive objects.
*   Minimize allocations in hot paths.
*   Use goroutines for independent pack processing with proper synchronization.
*   Always use `context` for cancellation.

### 10. Security Considerations

*   Validate all user input and sanitize file paths.
*   Check for directory traversal attempts.
*   Verify pack ownership and permissions.
*   Use secure temp file creation and validate symlink targets.

Remember: The goal is to write code that is safe, testable, and maintainable. When in doubt, refer to the design documents in `docs/design/` for architectural decisions and rationale.
