# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Added

- **Install-script visibility: header block, `# status:` markers, and `--verbose`.**
  `dodot up` previously discarded install-script stdout/stderr entirely,
  so a long-running script looked frozen and a misbehaving one was
  undebuggable. Three additions, all targeting install scripts:
  - The script's leading comment block (contiguous `#`-prefixed lines
    after the optional shebang) is printed when the script starts, so
    the user sees what's about to run.
  - Lines on stdout matching `# status: <message>` (or `#status:`) are
    streamed live as progress markers while the script runs. The
    convention is tool-agnostic: the markers are just shell comments
    when the script is run by hand.
  - `dodot up --verbose` (reusing the existing global flag) streams the
    script's raw stdout/stderr in real time. On failure, captured
    stderr is dumped automatically even without `--verbose` so the
    error is debuggable.

- **`probe shell-init` warns on stale profiles.** Shell-init profiles are
  written by `dodot-init.sh` only when a new shell starts, so running
  `dodot probe shell-init` from a shell that pre-dates the most recent
  `dodot up` would silently display pre-edit timings and sources. Each
  successful `up` now records a unix timestamp at
  `<data_dir>/last-up-at`, and every `probe shell-init` view (single,
  `--runs`, `--history`) compares the most recent profile's filename
  timestamp against that marker. If the profile predates the last `up`,
  a banner names both timestamps and prompts the user to open a new
  shell. Closes #59.

- **Per-source stderr capture and drill-down view for shell-init.**
  Until now, when a sourced shell file emitted to stderr or exited
  non-zero, dodot showed only an exit-code count in `--history` — the
  actual error message was on the user's terminal at startup time and
  gone. The shell wrapper now redirects each `. file` stderr to a
  per-shell scratch, re-emits live to the TTY (preserving the existing
  breadcrumb), and on non-empty stderr appends a record to a sibling
  `profile-<id>.errors.log` next to the TSV. Empty-stderr sources stay
  on the fast path — one `[ -s ]` test of overhead. Two new views read
  the sidecar:
  - `dodot probe shell-init <pack>[/<file>]` drills into one target's
    history across recent runs, inlining captured stderr under each
    failed run.
  - `dodot probe shell-init --errors-only` lists every target with a
    non-zero exit somewhere in the window, sorted by failure count desc.

  The `dodot status` runtime-failure footnote was also upgraded to
  inline a stderr excerpt from the most recent failing run (when
  captured) and to point users at the per-file probe view instead of
  `--history` (which only shows aggregate counts).

### Changed

- **`probe shell-init --history` lists newest first.** Previously the
  history table was reversed to put the latest run nearest the prompt;
  in practice this was confusing because every other dated listing in
  the tool reads newest-first. Now `--history` matches that
  convention.

### Fixed

- **`dodot up` reconciles deleted source files.** `up` was additive only:
  handler `to_intents()` emitted intents from current source, but nothing
  scanned the datastore for orphan entries. A file deleted from a pack
  would leave its data link behind, so the regenerated init script kept
  sourcing a now-missing path (silently swallowed by the `[ -f ]` guard,
  or surfacing as a non-zero exit row when profiling was on). Cleanup
  required `down + up`. Now `up` wipes each pack's datastore state for
  every configuration-category handler (path, shell, symlink) before
  re-applying from current source. Provisioning handlers (install,
  homebrew) are deliberately excluded so their sentinels keep gating
  re-runs by content hash rather than source presence. Closes #58.
