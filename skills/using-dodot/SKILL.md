---
name: using-dodot
description: Manage dotfiles with dodot — deploy packs, check what's live, add or adopt config files, undo deployments. Use when the user wants to deploy/update their dotfiles, run dodot up/down/status, add a file to a pack, adopt an existing config file into dodot, check what dodot has deployed, or undo a deployment.
---

# Using dodot

dodot is a convention-based dotfiles manager. You operate an **existing** dotfiles
repo with it: deploy packs, inspect what's live, bring new files under management,
and tear deployments down. This skill is for *using* dodot, not authoring a repo
from scratch.

## Mental model

Four leading words carry everything:

- **pack** — a top-level directory in the dotfiles repo (`vim/`, `git/`, `work/`).
  The unit `up`/`down`/`status` act on. Every top-level dir is a pack *except*
  hidden dirs (e.g. `.git/`), names matching the default ignore globs
  (`node_modules`, `.DS_Store`, …), invalid pack names, and any dir holding a
  `.dodotignore` marker. `dodot list` shows exactly what's discovered.
- **handler** — what dodot does with a file once a rule matches it. The filename
  decides the handler (a `*.sh` is sourced, `bin/` joins `$PATH`, anything else is
  symlinked). See `HANDLERS.md`.
- **up / down** — `up` deploys a pack (creates symlinks, registers shell sources
  and `$PATH` entries, runs install scripts). `down` removes the deployment. Your
  repo is never touched by either — only the **datastore** (`~/.local/share/dodot/`)
  and the live targets in `$HOME`.
- **source vs live** — the repo file is the **source**; the deployed path is the
  **live** file. dodot links live → source, so for symlinked files *they are the
  same bytes*. This distinction drives the one rule you must not get wrong (below).

dodot has **no apply database and no drift**: the filesystem *is* the state. git is
the source of truth; `up` is idempotent — re-run it freely.

## The core loop

Always **status → act → status**. Never hand-create symlinks or edit the
datastore; let dodot do the work and verify with `status`.

1. **Orient.** Confirm the dotfiles root (`$DOTFILES_ROOT`, else the git toplevel,
   else cwd). Run `dodot status` to see each pack's rollup
   (`pending` / `deployed` / `error`) and its per-file labels — which are
   handler-specific (`sourced`, `in PATH`, `installed`, `older version`,
   `skipped`, `gated out`, …), not the same three words.
2. **Act.** Run the workflow below. When unsure of the effect, run with
   `--dry-run` first — every mutating command supports it.
3. **Verify.** Run `dodot status` again and confirm the change landed (`deployed`,
   no `error`).

## The one rule: when does a re-run of `up` happen?

Editing an already-deployed file → **no `dodot up` needed**. The live path is the
source via symlink; the edit is already live. (Programs that only read config at
startup still need their own reload — that's the program's behavior, not dodot's.)

**Adding or removing a source file → run `dodot up` again.** `up` reconciles
per-pack state: new sources get linked, removed sources get their stale links
cleaned. This is the mistake to avoid — dropping a file into a pack and assuming
it's live. It isn't until the next `up`.

Install / Homebrew / Nix scripts are the exception in the other direction: editing
them does **not** auto-rerun (too destructive to do silently). `status` flags them
as "older version"; apply with `dodot up --provision-rerun`.

## Workflows

### Deploy (or redeploy after a git pull)

```bash
dodot status                 # see current state
dodot up --dry-run           # preview when unsure
dodot up                     # deploy all packs (or: dodot up <pack>...)
dodot status                 # confirm deployed
```

### Add a new file to a pack

Put the file in the pack directory, named so the right handler claims it (see the
table below / `HANDLERS.md`), then re-run `up`:

```bash
cp ~/some/config ~/dotfiles/nvim/        # source now in the pack
dodot up nvim                            # link it live
dodot status nvim
```

### Adopt an existing live config into dodot

`adopt` moves a real file into a pack and replaces the original with a symlink back
— the inverse of dropping a file in by hand. Use it to bring already-existing
config under management without losing the live file:

```bash
dodot adopt ~/.gitconfig --into git      # explicit pack
dodot adopt ~/.config/helix/             # pack inferred from path
dodot status
```

### Undo a deployment

```bash
dodot down nvim                          # remove links/registrations for one pack
dodot down                               # or all packs; repo stays intact
dodot up nvim                            # redeploy later, any time
```

### Create a pack scaffold

```bash
dodot init <pack>                        # make the dir + starter .dodot.toml
dodot fill <pack>                        # add starter install.sh/aliases.sh/Brewfile
```

## How filenames map to handlers (the essentials)

| At pack root            | Handler  | Result                                  |
|-------------------------|----------|-----------------------------------------|
| `*.sh` `*.bash` `*.zsh` | shell    | sourced at login                        |
| `bin/`                  | path     | directory added to `$PATH`              |
| `install.sh`            | install  | run once (tracked; won't re-run)        |
| `Brewfile`              | homebrew | `brew bundle`                           |
| `packages.nix`          | nix      | `nix profile install`                   |
| `README` `LICENSE` …    | skip     | not deployed; shown as `skipped`        |
| anything else           | symlink  | linked to `~/.<name>` or `~/.config/<pack>/` |

Routing prefixes on a symlinked file override the default target: `home.X` →
`~/.X`, `xdg.X` → `$XDG_CONFIG_HOME/X`, `app.X` → app-support dir; the directory
forms `_home/`, `_xdg/`, `_app/` route a whole subtree. Full handler behavior,
filter handlers (ignore/skip/gate), and per-handler liveness are in `HANDLERS.md`.

## Going deeper

- **`HANDLERS.md`** — every handler, the default rules and priorities, filter
  handlers, routing prefixes, and what propagates live vs needs another `up`.
- **`CLI.md`** — full command and flag reference, including `probe`
  (introspection) and `config`. Reach for it when a command or flag here isn't enough.
- **dodot-templates skill** — if the repo uses `*.tmpl` templates, `{{ secret(...) }}`,
  or `*.age`/`*.gpg` files. There the source is **not** the deployed bytes, which
  changes the editing rules; don't treat those files like ordinary symlinks.

When behavior is ambiguous, the per-command help is authoritative: `dodot <cmd> --help`.
