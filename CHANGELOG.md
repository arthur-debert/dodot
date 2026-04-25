# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.22.0] - 2026-04-25

### Added

- **Interactive `dodot tutorial` subcommand.** Walks new users through
  their first pack deployment using their actual dotfiles repo. A
  hand-rolled state machine over 12 named steps renders templated
  bodies (via `standout-render`) and asks one question per step
  through a `Prompts` trait. `InquirePrompts` (production) wraps
  `inquire` with italic prompt styling tied to a new `tutorial-prompt`
  theme key; `ScriptedPrompts` (tests) feeds canned answers, surfacing
  wizard-reorder bugs at the offending step. `TutorialEnv` bundles
  fs/paths/datastore/config so tests run against a `TempEnvironment`
  fixture instead of process env. Branches handle the empty-repo case
  (offers `dodot init`), config-only packs (skip the shell-integration
  step), and the eval-line prompt (append/clipboard/skip). Resume
  state lives at `$XDG_DATA_HOME/dodot/tutorial.json` and is cleared
  on completion.

## [0.21.0] - 2026-04-25

### Added

- **Pack ordering: numeric-prefix grammar.** Pack directories matching
  `^(\d+)[-_](.+)$` (e.g. `010-brew`, `100_zsh`, `900-starship`) now
  have their prefix recognised as ordering metadata. The full
  directory name remains the sort key (so prefixed packs apply in
  lex order), but the stem after the separator is treated as the
  pack's *display name* — what `dodot status`, `dodot list`, error
  messages, generated shell-init comments, and log lines all use.
  CLI arguments resolve against the display name first
  (`dodot up brew`) and fall back to the raw on-disk directory name
  (`dodot up 010-brew`). Symlink targets follow the display name too,
  so `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua`. Three
  classes of collision are rejected at scan time with both offending
  paths in the error: logical-name (`nvim` + `010-nvim`),
  multi-prefix (`010-nvim` + `020-nvim`), and empty-stem (`010-` /
  `010_`). The internal datastore subtree continues to be keyed by
  the raw on-disk directory name.

### Changed

- Documented the cross-pack ordering contract: dodot processes packs in
  lexicographic order of their on-disk directory names, and that order
  determines shell init source order, `$PATH` entry order, and
  install/homebrew execution order. Added a "Cross-Pack Ordering"
  section to `docs/reference/handlers.lex` and a bootstrap-zone note to
  `docs/user/getting-started.lex` covering what belongs above the
  `eval "$(dodot init-sh)"` line.
- `dodot status`, `dodot list`, `dodot up`, and `dodot down` output
  now displays the stripped form for prefixed packs (a pack on disk
  as `010-nvim` shows as `nvim`). Scripts that referenced the raw
  form continue to work via the lookup fallback.

## [0.20.0] - 2026-04-25

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

## [0.19.0] - 2026-04-25

### Added

- **`dodot probe`** — a new diagnostics command tree for introspecting
  deployed state. Three subcommands:
  - `dodot probe` — summary listing the available probe subcommands.
  - `dodot probe deployment-map` — rendered `pack / handler / kind /
    source / datastore` table derived live from the datastore. Paths are
    shortened to `~/…` where possible.
  - `dodot probe show-data-dir [--depth N]` — bounded-depth tree view of
    `<data_dir>` with per-node sizes and symlink targets. Truncated
    subtrees report `(… N more)` so nothing disappears silently. Default
    depth is 4; symlinks are never followed. Directories sort before
    files.

  All three honour `--output json` and emit a `{"kind": "…"}`-tagged
  document for programmatic consumers.
- **Deployment map file.** `dodot up` and `dodot down` now also write
  `<data_dir>/deployment-map.tsv` alongside the regenerated shell init
  script. The file is plain-text TSV with a `# dodot deployment map v1`
  header, one row per datastore entry (`pack\thandler\tkind\tsource\tdatastore`),
  overwritten on every run. Skipped during `--dry-run`. `dodot probe
  deployment-map` renders its table live from the datastore, not from
  this file; the TSV is a written snapshot for machine-to-machine
  consumers, including the forthcoming `dodot refresh` (see
  `docs/proposals/magic.lex`), which will use it for source-template
  mtime touches.
- `Pather::deployment_map_path()` trait method, returning
  `<data_dir>/deployment-map.tsv`.
- **Shell-init profiling** (Phase 2 of `docs/proposals/profiling.lex`).
  When `[profiling] enabled = true` (the default) the generated
  `dodot-init.sh` carries a runtime-detected timing wrapper around each
  `source` and `PATH` line. On bash 5+ / zsh, every shell startup writes
  one TSV under `<data_dir>/probes/shell-init/profile-<unix_ts>-<pid>-<rand>.tsv`
  with microsecond `EPOCHREALTIME` start/end pairs and the source's exit
  status — turning silent failures in user shell scripts into visible
  data. Older shells (`/bin/sh`, bash <5) fall through to the
  unmodified source/PATH path with one extra `[ "$_dodot_prof" = "1" ]`
  comparison of overhead. Disable via `[profiling] enabled = false` in
  the root `.dodot.toml`; the resulting init script is byte-identical to
  Phase 1.
- **`dodot probe shell-init`** — reads the most recent profile TSV and
  renders it grouped by pack and handler, with per-row durations,
  per-group subtotals, and a final user-sourced / dodot-framing / grand
  total breakdown. Falls back to a hint when no profile has been written
  yet, or when profiling is disabled in config.
- New `[profiling]` config section (root-only): `enabled` (default
  `true`) and `keep_last_runs` (default `100`, capped at the
  configured number per `dodot up`'s rotation pass; `0` disables
  rotation defensively rather than wiping history).
- `Pather::probes_shell_init_dir()` trait method, returning
  `<data_dir>/probes/shell-init`.

### Changed

- Shell-related handlers now recognize `.bash` and `.zsh` extensions in
  addition to `.sh`. The install handler's default claims are
  `install.{sh,bash,zsh}` and the shell handler's defaults cover
  `{aliases,profile,login,env}.{sh,bash,zsh}`. The install interpreter is
  selected from the script's extension (`.zsh` → `zsh`, otherwise `bash`),
  not from the user's login shell — the extension is the contract the
  pack author declares.
- `[mappings] install` in `.dodot.toml` now takes a list of patterns
  (e.g. `install = ["install.sh", "install.bash"]`) instead of a single
  string, matching the shape of `[mappings] shell`.

## [Unreleased]

### Changed

- **Intra-pack handler execution order is now explicit.** Previously ordering was `category → alphabetical by handler name`, which happened to produce the right sequence (homebrew, install, path, shell, symlink) but was fragile — adding a handler with a name sorted earlier alphabetically would have silently reordered the pipeline. Handlers now declare an `ExecutionPhase` (`Provision` → `Setup` → `PathExport` → `ShellInit` → `Link`), and `rules::handler_execution_order` sorts on the enum's declared order. The observable order is unchanged; the contract is now encoded in the type system, and adding a handler requires a deliberate choice of phase. `HandlerCategory` (used by `--no-provision`) is derived from phase. Catchall-last is now enforced by `Link` being the final variant rather than by convention.

## [0.18.4] - 2026-04-24

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

## [0.18.3] - 2026-04-23

### Changed

- Errors are now surfaced as indexed footnotes instead of being spliced into the per-file status column. Each item row stays one line with a short status label (`pending`, `error`, `stale`, …); long error bodies, stderr, and conflict reasons all render in a dedicated `Errors:` section at the bottom, referenced from the row by a `[N]` marker. Indices are command-wide, so the same scheme covers `status`, `up`, and `adopt` with one rendering path. Replaces the previous per-pack footnote mechanism and the "append a raw error row at the end of the pack" hack.

### Fixed

- `dodot up` now renders the full per-pack listing and notes section when a cross-pack conflict blocks deployment, matching what `dodot status` shows. Previously the CLI handler hardcoded an empty pack list on the cross-pack conflict branch, so users only saw the trailing conflicts dump and lost all context about what *would* have been deployed.

## [0.18.1] - 2026-04-23

### Fixed

- Symlink deploys now refuse when an ancestor of the target path is a symlink resolving into `dotfiles_root` or `data_dir`. Writing through such an ancestor landed back inside the pack store — silently clobbering pack source files (top-level files built a pack↔data-dir cycle) or surfacing as a misleading `non-symlink file at target path` (pack directories). The check runs in both real and dry-run modes; `--force` does not bypass it. Relative ancestor targets like `~/.config/warp -> ../dotfiles/warp` are lexically normalized before the prefix comparison so they get caught too.

## [0.18.0] - 2026-04-23

### Changed

- **BREAKING:** Symlink handler now deploys every pack-root entry — file or directory — to `$XDG_CONFIG_HOME/<pack>/<name>` by default (#48). Previously, top-level files defaulted to `$HOME/.<name>` and top-level directories to `$XDG_CONFIG_HOME/<name>` (no pack namespace). The new rule is consistent across files and dirs and matches modern tool conventions (nvim, helix, ghostty, kitty, alacritty, lazygit, starship, …) without forcing users to write `pack/program/` doubled paths.
- **BREAKING:** Per-file `$HOME` opt-in convention renamed: `dot.X` → `home.X`. The semantic is unchanged (`<pack>/home.bashrc` → `~/.bashrc`); the new name reads as "deploy to home as .X" instead of "this filename has a literal dot." All `[symlink.targets]`, `_home/`, `_xdg/`, `force_home`, and `protected_paths` overrides keep their existing semantics.
- The `_home/` and `_xdg/` directory prefixes are now always per-file (never wholesale-linked at the top level) — wholesale-linking the prefix dir itself would have baked the literal `_home`/`_xdg` segment into the deploy path, which is never what users meant.

### Migration notes (#48)

- A pack with `git/gitconfig` previously deployed to `~/.gitconfig`. It now deploys to `~/.config/git/gitconfig` (which git itself reads via XDG since 2.20). To keep the legacy `$HOME` path, rename the file to `git/home.gitconfig` (per-file home opt-in) or add a `[symlink.targets]` override.
- A pack with `warp/themes/` previously deployed to `~/.config/themes`. It now deploys to `~/.config/warp/themes`. Pin consumers to the new path or use `_xdg/themes/` inside the pack to skip the namespace.
- A pack with `git/dot.gitconfig` (old per-file convention) needs to be renamed to `git/home.gitconfig` — `dot.X` is no longer recognized.
- A pack containing literal `config/` or `.config/` subdirectory paths (e.g. `app/config/main.toml`) used to have that prefix silently stripped during resolution to avoid the `$XDG_CONFIG_HOME/.config/...` double-prefix. Under #48 the strip is gone; the file deploys to `$XDG_CONFIG_HOME/<pack>/config/main.toml` literally. If you relied on the old strip to land at `$XDG_CONFIG_HOME/main.toml`, move the file to the pack root (`app/main.toml` → `~/.config/app/main.toml`) or use `_xdg/main.toml` to skip the pack namespace entirely.
- No change for files matching `force_home` (ssh, gpg, bashrc, zshrc, profile, inputrc, etc.) — those still deploy to `$HOME/.<name>`.
- No change for files routed via `[symlink.targets]`, `_home/`, or `_xdg/` directory prefixes — those keep their existing behavior.

## [0.16.0] - 2026-04-23

### Added
- Provisioning script execution now prints `==== <pack> → <handler> → <script>  running…` before the spawn and `OK` (green) or `FAILED` (red) after, on stderr. Sentinel-skipped runs stay silent. Long-running scripts (e.g. brew installs) are no longer opaque from the user's side.

### Changed
- **`up` and `down` now render through `status::status()`** instead of using their own per-operation vocabulary. `dodot up video` and `dodot status video` produce identical right-column labels (`in PATH`, `sourced`, `deployed`) for the same observed state — previously `up` reported `staged bin` while `status` reported `in PATH`, which was confusing. Operation failures from `up` are overlaid as additional error rows on top of the status view. Dry-run output is unchanged (still shows planned operations). (#42)
- **`status` distinguishes "pending — clear to deploy" from "pending — would conflict on deploy"** (#43). When a non-symlink file or directory already occupies a symlink-handler target path, status renders the row with the `warning` style (label remains `pending` so the right column stays compact) and adds a per-pack footnote `(N) <path> (existing file/directory) — \`dodot up\` will refuse without --force`. Pre-existing symlinks (correct, dangling, or pointing elsewhere) are *not* flagged as conflicts because the executor's `create_user_link` gracefully replaces them on the next `up`.
- **`dodot up` auto-replaces content-equivalent pre-existing files** (#44). When `up` would deploy a symlink to a path where a regular file already exists, it now checks whether the file's content is byte-identical to the source. If so, it silently swaps the file for the dodot symlink chain — no `--force` required, no conflict reported. Direct (single-hop) symlinks pointing at the source — including relative-path symlinks — were already handled gracefully by `create_user_link`; this completes the picture for the file case. Multi-hop symlink chains are still replaced automatically (unchanged behavior). Only content-different non-symlink files still require `--force` — mismatched content is a real conflict.
- **`status` no longer flags content-equivalent files as conflicts** (#44). A pre-existing file at the user-target path whose bytes match the source is rendered as plain `pending` with no footnote, since `up` will handle it without `--force`.
- **`dodot adopt` distinguishes "fully managed" from "direct symlink to pack source"** (#44). When the user's existing symlink points directly into the dotfiles root (skipping dodot's data-link layer), adopt now skips with a clearer message that points at `dodot up <pack>` to upgrade to the full chain — instead of the previous opaque `already managed by dodot`. Sources whose symlinks already go through dodot's data dir keep the original wording.

## [0.14.0] - 2026-04-22

### Added
- **Cross-pack conflict detection** (#29): `dodot up` now collects intents from all packs before executing any, detects when multiple packs produce symlinks targeting the same resolved path, and halts with a clear error listing conflicting packs, handlers, and source files — no partial deployment occurs
- `dodot status` surfaces potential cross-pack conflicts as warnings, even for packs that aren't deployed yet
- Symlink target collisions detected across all resolution layers: `[symlink.targets]`, `_home/` prefix, `dot.` convention, `force_home`, XDG defaults
- PATH executable shadowing detected: two packs with `bin/` directories containing same-named files are flagged (one would shadow the other in `$PATH`)
- **Auto-executable permissions**: `dodot up` now automatically adds `+x` to files in path-handler directories (`bin/`), matching user intent that these files should be runnable. Controlled by `[path] auto_chmod_exec` (default: `true`). Already-executable files are left untouched; permission failures are reported as warnings, not hard errors.
- New `CrossPackConflict` error variant with structured conflict data
- `dodot status` now lists directories skipped via `.dodotignore` under an "Ignored Packs" heading, so users aren't baffled when a pack-shaped directory doesn't appear in the main listing

### Changed
- **`mappings.shell` default now includes `env.sh`** alongside `aliases.sh`, `profile.sh`, `login.sh`. Files named `env.sh` in any pack are now claimed by the shell handler (sourced at shell init) instead of falling through to the symlink handler (which previously dropped them at `~/.env.sh` and collided across packs)
- `dodot up` now uses a two-phase execution model: collect all intents first, then execute — replacing the previous sequential per-pack execution
- `--force` does not override cross-pack conflicts (it only applies to pre-existing non-dodot files); cross-pack conflicts require a configuration fix
- Orchestration pipeline split into `collect_pack_intents()` and `execute_intents()` for composability
- **Scanner is top-level only** (#37): rules match pack depth-1 entries only; nested files are the responsibility of the handler that owns the containing top-level entry. Fixes two long-standing issues:
  - A pack's `bin/tool` was being **both** staged via the path handler *and* symlinked individually via the catchall — now only the `bin/` directory is claimed (by path). One shell command, one status line.
  - A nested `foo/install.sh` used to trigger the install handler because matching was on basename-only; now only a top-level `install.sh` does.
- **Symlink handler links top-level directories wholesale**: `warp/themes/` now becomes a single symlink `~/.config/themes → <pack>/warp/themes`, not a per-file listing. Falls back to per-file mode (current behavior) when `[symlink] protected_paths` or `[symlink.targets]` reach inside the directory, preserving every existing security guarantee.
- **Top-level dirs default to `$XDG_CONFIG_HOME/<name>`** (aligning code with the longstanding docs). Top-level *files* still default to `$HOME/.<name>`. `force_home`, `_home/`, `_xdg/`, `dot.`, and `targets` overrides all still apply.
- Handler trait gains `match_mode()` (`Precise` / `Catchall`) and `scope()` (`Exclusive` / `Shared`), with a registry invariant that at most one handler may be simultaneously `Catchall` + `Exclusive`. No behavior change for existing handlers; the formalization future-proofs adding non-claiming observers.
- `Handler::to_intents` now receives an `&dyn Fs` so handlers can inspect directory contents when deciding wholesale-vs-per-file treatment.

### Migration notes
- If you previously relied on nested files being individually symlinked (e.g. `warp/themes/nord.yaml → ~/.config/themes/nord.yaml`), the whole `themes/` directory is now one symlink. The observed path `~/.config/themes/nord.yaml` still resolves identically via the directory link.
- If you need nested per-file behavior (different targets per file, or selective inclusion), add `[symlink.targets]` entries or list individual files in `[symlink] protected_paths` — either triggers per-file mode for the containing directory.
- If you had nested `install.sh` / `aliases.sh` / `Brewfile` that were (perhaps unintentionally) being picked up by their handlers, move them to the pack's top level or use `[mappings]` overrides.

## [0.9.3] - 2026-04-16

### Added
- Structured logging via `tracing` with daily-rotating file output to `~/.cache/dodot/logs/`
- `--verbose` flag: show INFO-level log messages on stderr
- `--debug` flag: show DEBUG-level log messages on stderr
- INFO and DEBUG events across orchestration pipeline and executor subsystems
- Automatic cleanup of log files older than 7 days

## [0.9.2] - 2026-04-16

### Changed
- CI: force Node.js 24 for all GitHub Actions (future-proofing for June 2026 deprecation)
- CI: replace manual `actions/cache` with `Swatinem/rust-cache` for smarter Rust caching
- CI: e2e tests now download pre-built binary from check job instead of rebuilding from source

## [0.9.1] - 2026-04-15

### Fixed
- Release workflow: add MIT license and crate metadata required by crates.io
- Release workflow: macOS signing failures no longer block binary packaging/upload
- Release workflow: fix cross-compilation install on runners with pre-existing `cross` binary

## [0.9.0] - 2026-04-15

### Added
- `[pack] ignore` now ships with sensible compiled defaults (`.git`, `.svn`, `.DS_Store`, `*.swp`, etc.) — previously the default was an empty list
- Exhaustive unit test verifying all compiled default values for `pack.ignore`, `symlink.force_home`, `symlink.protected_paths`, and all `mappings` fields

### Changed
- **BREAKING:** `[mappings] ignore` renamed to `[mappings] skip` to disambiguate from `[pack] ignore`
- Removed `genconfig` command — use `dodot config gen` (via clapfig) instead, which auto-generates a commented TOML template from struct definitions

### Fixed
- Config docs: removed phantom "App Defaults" and "App Config" layers that never existed in code; documented the actual 3-layer hierarchy (compiled defaults → root `.dodot.toml` → pack `.dodot.toml`)
- Config docs: corrected merge semantics from "arrays append" to "arrays override (last value wins)" matching actual clapfig behavior
- Config docs: fixed `[symlink]` targets syntax from incorrect bare keys to correct `[symlink.targets]` table form
- Synced `force_home` defaults in docs (was 6 entries, now all 10 matching code)
- Synced `protected_paths` defaults across both doc files and code (was inconsistent between `config-system.lex` and `symlink-paths.lex`)

## [0.1.0] - 2026-04-14

### Added

- Core dotfiles management with pack-based organization
- Symlink handler with smart path resolution (home dotfiles, XDG config, custom targets)
- Install handler with content-based checksums for idempotent script execution
- Homebrew handler for Brewfile-driven installs
- Shell handler for sourcing shell configuration files
- Path handler for adding directories to PATH
- Commands: `up`, `down`, `status`, `list`, `init`, `adopt`, `fill`, `addignore`, `init-sh`, `config`
- Pack discovery with `.dodotignore` support
- Configuration system: defaults, root `.dodot.toml`, per-pack `.dodot.toml` overrides
- `dot.` prefix convention for top-level pack files
- Per-file custom symlink target overrides
- Double-link datastore architecture for state tracking
- Shell init script generation (`eval "$(dodot init-sh)"`)
- Dry-run mode for all deployment commands
- JSON output mode for scripting
- Themed terminal output via standout
