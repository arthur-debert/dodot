# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use sections: Added, Changed, Deprecated, Removed, Fixed, Security.

### Added
- **Cross-pack conflict detection** (#29): `dodot up` now collects intents from all packs before executing any, detects when multiple packs produce symlinks targeting the same resolved path, and halts with a clear error listing conflicting packs, handlers, and source files — no partial deployment occurs
- `dodot status` surfaces potential cross-pack conflicts as warnings, even for packs that aren't deployed yet
- Symlink target collisions detected across all resolution layers: `[symlink.targets]`, `_home/` prefix, `dot.` convention, `force_home`, XDG defaults
- PATH executable shadowing detected: two packs with `bin/` directories containing same-named files are flagged (one would shadow the other in `$PATH`)
- **Auto-executable permissions**: `dodot up` now automatically adds `+x` to files in path-handler directories (`bin/`), matching user intent that these files should be runnable. Controlled by `[path] auto_chmod_exec` (default: `true`). Already-executable files are left untouched; permission failures are reported as warnings, not hard errors.
- New `CrossPackConflict` error variant with structured conflict data

### Changed
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
