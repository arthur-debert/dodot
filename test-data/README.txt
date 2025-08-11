                        dodot Live System Tests
                        =======================

This directory contains the live system integration tests for dodot. These tests
run in a Docker container to safely test file system operations without risking
damage to the host system.

Test Organization
-----------------

tests are organized into suites under the scenarios/ directory:

- suite-1-single-powerups/     Single power-up tests (basic functionality)
- suite-2-multi-powerups-single-pack/   Multiple power-ups in one pack
- suite-3-multi-powerups-multi-packs/   Multiple power-ups across packs  
- suite-4-single-powerup-edge-cases/    Edge cases for individual power-ups
- suite-5-complex-edge-cases/           Complex multi-pack scenarios
- test-framework/               Tests for the test framework itself

Running Tests
-------------

Always run tests from the project root using the container script:

    # Run all tests with human-friendly output
    ./containers/dev/run-tests-native.sh
    
    # Run specific suite
    ./containers/dev/run-tests-native.sh test-data/scenarios/suite-1-single-powerups/**/*.bats
    
    # Run specific test file
    ./containers/dev/run-tests-native.sh test-data/scenarios/suite-1-single-powerups/path/tests/path.bats
    
    # Run tests matching pattern
    ./containers/dev/run-tests-native.sh --filter "path: YES"
    
    # Generate JUnit XML output for CI
    ./containers/dev/run-tests-native.sh --formatter junit
    
    # Show help
    ./containers/dev/run-tests-native.sh --help

Output Formats
--------------

The test runner supports multiple output formats via Bats native formatters:

- Default: Human-friendly output with suite grouping and summary
- --formatter junit: JUnit XML saved to test-results.xml
- --formatter tap: TAP format for CI systems  
- --formatter tap13: TAP v13 with timing information
- --formatter pretty: Bats default colored output

The JUnit XML report is always saved to the project root as test-results.xml
and is accessible from the host system for CI processing.

Test Implementation
-------------------

Tests use the Bats testing framework with custom assertion libraries:

- lib/assertions.sh             Basic assertions (file_exists, etc.)
- lib/assertions_*.sh           Power-up specific assertions
- lib/setup.sh                  Test environment setup/teardown
- lib/debug.sh                  Debug output on test failures
- lib/common.sh                 Common test setup that loads all libraries

Key Components
--------------

- runner-native.sh              Simplified test runner using Bats native features
- junit-summary.py              Python script to format JUnit XML output
- containers/dev/run-tests-native.sh    Host-side script to run tests in container

Safety Features
---------------

- Tests ONLY run inside Docker container (enforced by runner)
- Each test gets isolated temporary directories
- Automatic cleanup after each test
- Cannot modify host file system

Writing New Tests
-----------------

See docs/dev/40_live-testing.txxt for detailed information on:
- Test structure and conventions
- Available assertion functions  
- Creating new test scenarios
- Debugging test failures