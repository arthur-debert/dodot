# CLI reference

Per-command help is authoritative: `dodot <cmd> --help`. Every mutating command
supports `--dry-run`.

## Daily commands

### `dodot status [PACKS...]`

Read-only. Per-file: handler symbol, live target, state (`pending` / `deployed` /
`error`), plus `skipped` files. Symbols: `➞` symlink · `⚙` shell/homebrew · `+`
`$PATH` · `×` install.

- `--check-drift` — hash deployed external files, report divergence (opt-in, slow).
- `--diff` — for provisioning files reporting "older version", show the unified diff.
- `--full` / `--short` — per-file detail vs one line per pack (default `--full`).
- `--by-name` / `--by-status` — sort order (default `--by-name`).

### `dodot up [PACKS...]`

Deploy: materialize symlinks, register shell sources and `bin/` on `$PATH`, run
provisioning when its content hash changed. Phases: plan → detect cross-pack
conflicts (stops if any) → execute (wipe each pack's stored state, re-apply from
source). Idempotent.

- `--dry-run` — preview only.
- `--no-provision` — skip install scripts and Brewfile.
- `--provision-rerun` — force-rerun provisioning even if the sentinel matches.
- `--force` — overwrite pre-existing files at target locations.

### `dodot down [PACKS...]`

Remove deployments: delete symlinks, clear shell-source and `$PATH` registrations,
remove provisioning sentinels. The dotfiles repo is untouched.

- `--dry-run`.

## Pack management

### `dodot list`

List discovered packs (display names; ordering prefixes stripped). Skips dirs with
`.dodotignore` and the default ignore globs (`.git`, `node_modules`, `.DS_Store`, …).

### `dodot init <PACK>`

Create `<root>/<PACK>/` and a commented starter `.dodot.toml`. No handler files
(use `fill`). Errors if the dir exists.

### `dodot fill <PACK>`

Add starter handler files to an existing pack — `install.sh` (0755), `aliases.sh`,
`Brewfile` — each substituting the pack name. Never overwrites existing files.

### `dodot adopt <FILES...>`

Move existing config into a pack and replace the original with a symlink back.

- `--into <PACK>` — force the destination pack (must exist; overrides inference).
- `--force` — overwrite existing destination files in the pack.
- `--no-follow` — move the symlink itself, not its target.
- `--dry-run`.
Pack is inferred from the source path when `--into` is omitted: `$XDG_CONFIG_HOME/X/…`
→ pack `X`; bare `~/.X` files/dirs generally require `--into`.

### `dodot addignore <PACK>`

Drop a zero-byte `.dodotignore` so the directory stops being discovered as a pack.
Idempotent; reverse with `rm <pack>/.dodotignore`.

## Shell integration

- `dodot init-sh` — print the shell init script; add `eval "$(dodot init-sh)"` to
  `~/.zshrc` / `~/.bashrc`.
- `dodot git-show-alias` / `dodot git-install-alias` [`--shell SHELL`] — the git
  wrapper alias that runs `dodot refresh --quiet` so `git status`/`git diff` see
  deployed-side template edits (show vs write-to-rc).

## Introspection — `dodot probe`

Read-only, lower-level than `status`.

- `deployment-map` — every symlink dodot created (source → live); machine-readable,
  supports `--output json`.
- `show-data-dir [--depth N]` — tree of the datastore (`~/.local/share/dodot`), by
  pack + handler (default depth 4).
- `shell-init [<PACK[/FILE]>] [--runs [N]] [--history] [--errors-only]` — per-source
  shell-startup timings, exit codes, stderr.
- `app <PACK> [--refresh]` (macOS) — app-support folders, matching cask, bundle id.

## Configuration — `dodot config`

- `list` — resolved config values · `get <KEY>` — one key with its docs · `set
  <KEY> <VALUE>` · `unset <KEY>` · `gen [-o FILE]` — print/write a fully-commented
  `.dodot.toml` starter.

`.dodot.toml` lives at the repo root (all packs) and/or per-pack (that pack only);
pack config layers over root. Key sections: `[mappings]` (handler dispatch),
`[symlink]` (target routing), `[path]`, `[preprocessor.template.vars]`, `[secret]`,
`[gates]`, `[pack]`.

## Templates, secrets & their git integration

Out of scope here — these are a separate concern with their own footguns (the
source is **not** the deployed bytes). If a repo uses `*.tmpl`/`*.template`,
`{{ secret(...) }}`, or `*.age`/`*.gpg` files, or you need `dodot refresh` /
`dodot transform` / `dodot secret`, use the **dodot-templates** skill.

`dodot plist clean|smudge` + `git-install-filters` (binary↔XML plist git filters,
macOS) are git plumbing — install once per clone and ignore; `dodot
git-install-filters` writes them.

## Misc

- `dodot tutorial [--reset] [--from STEP]` — interactive walkthrough on the real repo.
- `dodot prompts list` / `reset [KEY] [--all]` — manage one-shot CLI prompts.
