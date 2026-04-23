# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use sections: Added, Changed, Deprecated, Removed, Fixed, Security.

### Added
- Provisioning script execution now prints `==== <pack> → <handler> → <script>  running…` before the spawn and `OK` (green) or `FAILED` (red) after, on stderr. Sentinel-skipped runs stay silent. Long-running scripts (e.g. brew installs) are no longer opaque from the user's side.

### Changed
- **`up` and `down` now render through `status::status()`** instead of using their own per-operation vocabulary. `dodot up video` and `dodot status video` produce identical right-column labels (`in PATH`, `sourced`, `deployed`) for the same observed state — previously `up` reported `staged bin` while `status` reported `in PATH`, which was confusing. Operation failures from `up` are overlaid as additional error rows on top of the status view. Dry-run output is unchanged (still shows planned operations). (#42)
- **`status` distinguishes "pending — clear to deploy" from "pending — would conflict on deploy"** (#43). When a non-symlink file or directory already occupies a symlink-handler target path, status renders the row with the `warning` style (label remains `pending` so the right column stays compact) and adds a per-pack footnote `(N) <path> (existing file/directory) — \`dodot up\` will refuse without --force`. Pre-existing symlinks (correct, dangling, or pointing elsewhere) are *not* flagged as conflicts because the executor's `create_user_link` gracefully replaces them on the next `up`.
- **`dodot up` auto-replaces content-equivalent pre-existing files** (#44). When `up` would deploy a symlink to a path where a regular file already exists, it now checks whether the file's content is byte-identical to the source. If so, it silently swaps the file for the dodot symlink chain — no `--force` required, no conflict reported. Direct (single-hop) symlinks pointing at the source — including relative-path symlinks — were already handled gracefully by `create_user_link`; this completes the picture for the file case. Multi-hop symlink chains are still replaced automatically (unchanged behavior). Only content-different non-symlink files still require `--force` — mismatched content is a real conflict.
- **`status` no longer flags content-equivalent files as conflicts** (#44). A pre-existing file at the user-target path whose bytes match the source is rendered as plain `pending` with no footnote, since `up` will handle it without `--force`.
- **`dodot adopt` distinguishes "fully managed" from "direct symlink to pack source"** (#44). When the user's existing symlink points directly into the dotfiles root (skipping dodot's data-link layer), adopt now skips with a clearer message that points at `dodot up <pack>` to upgrade to the full chain — instead of the previous opaque `already managed by dodot`. Sources whose symlinks already go through dodot's data dir keep the original wording.
