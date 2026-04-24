# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use sections: Added, Changed, Deprecated, Removed, Fixed, Security.

### Added

- **`--short` / `--full` output modes** for every command that renders pack status (`status`, `up`, `down`, `adopt`). `--short` collapses each pack to a single summary line (`git  (2) error`, `nvim  (3) deployed`) showing the count of files in the pack's worst-status bucket. `--full` keeps today's per-file listing and is the default. Flags are global on the root `dodot` command and mutually exclusive.
- **`--by-status` / `--by-name` grouping modes** for the same four commands. `--by-status` groups packs under coloured banners — `Ignored Packs` / `Deployed Packs` / `Pending Packs` / `Error Packs`, top to bottom — so errors land closest to the cursor where the user's eye finishes reading. Empty banners are hidden. `--by-name` keeps flat discovery-order listing and is the default. The two flag pairs are composable (all four combinations are valid) and applied via a max-status rollup per pack: `error`/`broken` → error, `pending`/`warning`/`stale` → pending, `deployed` → deployed.
- `DisplayPack.summary_status` and `DisplayPack.summary_count` exposed in JSON output for programmatic consumers.

### Changed

- **dodot's CLI theme is now adaptive for light and dark terminals.** Previously the theme hardcoded light-mode colours (`.pack-name: #000`, `.pending` with a near-white background, dim chromatics like `#008700` green and `#005F87` cyan), which were invisible or barely legible on dark backgrounds. The theme now splits into a mode-agnostic base plus `@media (prefers-color-scheme: light|dark)` blocks: monochrome values invert between modes (black → white for `.pack-name`, light-grey → dark-grey backgrounds for `.pending`), and dim chromatic values brighten for dark mode (`#008700` → `#5FD75F`, `#D70000` → `#FF5F5F`, etc). Standout's `standout-render` auto-detects the terminal colour scheme and selects the right variant per-session.
- **Shared templates moved into `dodot-lib`.** The three pack-status-rendering templates (`pack-status.jinja`, `list.jinja`, `message.jinja`) now live under `crates/dodot-lib/src/templates/` and are exported as `pub const` strings from `dodot_lib::render`. The CLI registers them via `EmbeddedTemplates::new()` using those constants. Previously the CLI owned the template files and the lib kept a drifting string-literal copy; this eliminates the duplication while keeping `dodot-lib` self-contained (no cross-crate `include_str!`).

### Fixed

- Status rows in `broken` or `stale` states now render with proper styling instead of falling through to standout's `[broken?]text[/broken?]` unknown-tag marker. The two styles were missing from the CSS theme; they now render as red (broken) and amber (stale) with the same light/dark adaptation as the rest of the theme.
- `dodot --help` (and all subcommand help) no longer shows `[about?]`, `[usage?]`, `[item?]`, `[desc?]`, `[example?]` unknown-tag markers. Setting `.default_theme("dodot")` in standout replaces the built-in help theme wholesale rather than merging with it, so those five tags were unregistered in the active theme. Added them to `dodot.css` (matching standout's defaults: `item` bold, the rest plain).

### Removed

- Unused `config.jinja` template (was embedded as a side effect of directory scanning but never referenced by any handler).
