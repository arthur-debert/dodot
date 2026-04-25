# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use **level-3** section headings (`### Added`, `### Changed`, `### Deprecated`,
`### Removed`, `### Fixed`, `### Security`) so they nest cleanly under the
`## [version]` heading the release workflow inserts.

### Added

- **`dodot probe shell-init --runs N`** — aggregate the last N
  shell-startup profiles into a per-target table of `p50 / p95 / max`
  durations plus a `runs_seen / runs_total` ratio. Targets sort by
  `(pack, handler, target)` to match the deployment-map view.
  Percentiles use nearest-rank (no interpolation), which is the right
  resolution for sub-millisecond shell timings. When fewer than `N`
  profiles exist on disk the renderer warns "(requested N)" so the
  user can tell the data is sparse.
- **`dodot probe shell-init --history`** — one summary row per recent
  shell startup, oldest first, capped at 50 rows. Each row carries the
  parsed unix timestamp, shell label, total / user-sourced durations,
  entry count, and the count of entries with a non-zero
  `exit_status` — turning the once-silent "this source failed" signal
  into a single column on a trend view.
- Both new views honour `--output json` and serialize as
  `kind = "shell-init-aggregate"` / `kind = "shell-init-history"`.
  The JSON also includes raw `_us` fields alongside the humanised
  labels for programmatic consumers.
