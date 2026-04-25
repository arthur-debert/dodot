# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use sections: Added, Changed, Deprecated, Removed, Fixed, Security.

## Added

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

## Changed

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
