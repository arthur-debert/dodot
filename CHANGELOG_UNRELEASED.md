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
  overwritten on every run. Skipped during `--dry-run`. The TSV is what
  `dodot probe deployment-map` reads, and is also the file that the
  forthcoming `dodot refresh` (see `docs/proposals/magic.lex`) will
  consume for source-template mtime touches.
- `Pather::deployment_map_path()` trait method, returning
  `<data_dir>/deployment-map.tsv`.

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
