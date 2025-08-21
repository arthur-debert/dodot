                    Live Testing Infrastructure
                    ========================

This directory contains the complete live testing system for dodot, providing
comprehensive integration tests in a sandboxed Docker environment.

Directory Structure
-------------------

live-testing/
├── containers/     # Docker configurations and Dockerfiles
├── lib/            # Test assertion libraries and helper functions
├── scenarios/      # Test scenarios organized in 5 progressive suites
└── scripts/        # All executable scripts for building and running tests

Quick Start
-----------

From the project root:

    # Run all tests with pretty output
    ./scripts/run-live-tests-pretty

    # Run specific test suite
    ./scripts/run-live-tests live-testing/scenarios/suite-1/**/*.bats

    # Launch interactive development container
    ./scripts/run-dev-container

Test Organization
-----------------

Tests are organized in 5 progressive suites:

1. Suite 1: Single handlers (foundation tests)
2. Suite 2: Multiple handlers in single packs
3. Suite 3: Multiple handlers across multiple packs
4. Suite 4: Single handler edge cases
5. Suite 5: Complex multi-pack edge cases

See scenarios/test-plan.txt for detailed test specifications.

Key Components
--------------

• Assertion Libraries: Located in lib/, provide handler specific test helpers
• Test Runner: scripts/runner.sh orchestrates test execution with safety checks
• Docker Environment: Ubuntu-based container with Homebrew, Zsh, and Bats
• CI Integration: Outputs JUnit XML for GitHub Actions reporting

For comprehensive documentation, see docs/dev/40_live-testing.txxt