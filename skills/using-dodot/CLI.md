# CLI reference

Per-command help is authoritative: `dodot <cmd> --help`. Use a documented
preview where one exists; not every mutating command supports `--dry-run`.

## Daily commands

### `dodot status [PACKS...]`

Read-only. Per-file: handler symbol, live target, and a **handler-specific** label —
`pending`/`deployed` (symlink), `sourced`/`not sourced` (shell), `in PATH` (path),
`installed`/`never run`/`older version` (provisioning), `skipped`, `gated out`. Only
the **pack-level rollup** is constrained to `pending`/`deployed`/`error`. Symbols:
`➞` symlink · `⚙` shell/homebrew/nix · `+` `$PATH` · `×` install · `↓` external ·
`·` skip/gate (not deployed). `status --help` prints the same six.

Below the pack rows, one **shell hookup** line reports whether shells are actually
loading dodot: `shell hookup: ok`, `Deployed, but no shell has loaded dodot yet.`
(fix: `dodot install --write`), or `This shell started before your last dodot up.`
(fix: open a new shell). `status` reads evidence only and never starts a shell, so
it cannot report a measured verdict — `up` and `install --write` can.

- `--check-drift` — hash deployed external files, report divergence (opt-in, slow).
- `--diff` — for provisioning files reporting "older version", show the unified diff.
- `--full` / `--short` — per-file detail vs one line per pack (default `--full`).
- `--by-name` / `--by-status` — sort order (default `--by-name`).

### `dodot up [PACKS...]`

Deploy: materialize symlinks, register shell sources and `bin/` on `$PATH`, run
provisioning when its content hash changed. Phases: plan → detect cross-pack
conflicts (stops if any) → execute (wipe each pack's stored state, re-apply from
source). Idempotent. Safety Lock-gated: on an implicit unapproved root it prompts
(non-interactive: refuses, exit 1) — set `DOTFILES_ROOT` explicitly in automation.
The exhaustive protected set is `up`, `down`, `init`, `fill`, `adopt`,
`addignore`, root-persisted `config set` / `config unset`, mutating `refresh`,
`transform check`, `git-install-filters`, `template install-filter`,
`transform install-hook`, and the tutorial's real deployment step. Their
documented read-only or preview variants bypass the gate and establish no trust.
`install --write`, `git-install-alias`, prompt management, factory reset, and
stdin/stdout filter passthroughs are root-independent and remain outside it.

- `--dry-run` — preview only.
- `--no-provision` — skip every code-execution handler: install scripts, `Brewfile`,
  `packages.nix`, and `externals.toml`. Those files report `skipped (--no-provision)`.
- `--provision-rerun` — run provisioning against the file's current content. This is
  how you apply an edit: a plain `dodot up` never re-runs on its own, so an edited
  `install.sh` / `Brewfile` / `packages.nix` is reported as `older version` and held.
  It also re-runs content that has not changed at all.
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

- `dodot install` [`--write`] [`--rc FILE`] — wire dodot into the user's shell
  startup. Dry by default: bare, it reports the detected shell, the rc file it
  would write, whether a hook is already there, and the exact line. `--write`
  splices an idempotent marked block (`# >>> dodot shell hookup >>>`) and then
  starts a shell to verify the hook fires. bash/zsh only; any other shell is
  refused with the line to paste. Deleting the block is a complete uninstall.
- `dodot init-sh` — print the shell init script; the manual alternative is
  `eval "$(dodot init-sh)"` in `~/.zshrc` / `~/.bashrc`. Still supported; prefer
  `dodot install --write` unless the user wants to own the rc line.
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
