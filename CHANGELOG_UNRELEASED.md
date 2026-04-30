# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Fixed

- **CLI exits non-zero on handler errors.** Every standout-dispatched
  subcommand (`status`, `up`, `down`, `list`, `init`, `fill`, `adopt`,
  `addignore`, `probe …`) was printing `Error: …` to stdout and
  exiting **0** when the handler returned `Err`. Scripts piping with
  `&&` or CI invocations checking `$?` saw success on every failure
  path. The root cause was upstream in standout-dispatch: handler errors
  got stuffed into the success variant `RunResult::Handled(...)` and
  the binary couldn't tell them apart from real output. Fixed in
  standout 7.6.2 (arthur-debert/standout#141), which adds
  `RunResult::Error(String)`. dodot now matches that variant in
  `main.rs`, prints the message to stderr, and exits 1 — fixes every
  affected subcommand at once. A new `tests/e2e/bats/test_exit_codes.bats`
  pins the contract for `status`, `up`, `down`, and `adopt` (both
  pack-not-found and source-not-found shapes) so a future regression
  shows up immediately. Closes #86.
