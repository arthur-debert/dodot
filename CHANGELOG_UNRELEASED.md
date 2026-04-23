# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use sections: Added, Changed, Deprecated, Removed, Fixed, Security.

### Added
- Provisioning script execution now prints `==== <pack> → <handler> → <script>  running…` before the spawn and `OK` (green) or `FAILED` (red) after, on stderr. Sentinel-skipped runs stay silent. Long-running scripts (e.g. brew installs) are no longer opaque from the user's side.

### Changed
- **`up` and `down` now render through `status::status()`** instead of using their own per-operation vocabulary. `dodot up video` and `dodot status video` produce identical right-column labels (`in PATH`, `sourced`, `deployed`) for the same observed state — previously `up` reported `staged bin` while `status` reported `in PATH`, which was confusing. Operation failures from `up` are overlaid as additional error rows on top of the status view. Dry-run output is unchanged (still shows planned operations). (#42)
- **`status` distinguishes "pending — clear to deploy" from "pending — would conflict on deploy"** (#43). When a non-dodot file or symlink already occupies a symlink-handler target path, status renders the row with the `warning` style (label remains `pending` so the right column stays compact) and adds a per-pack footnote `(N) <path> (existing file/symlink/directory) — \`dodot up\` will refuse without --force`. Equivalent pre-existing symlinks (those already pointing at dodot's source) are also surfaced; auto-replace for them is the subject of #44.
