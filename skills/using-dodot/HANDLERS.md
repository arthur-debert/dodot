# Handlers reference

A **handler** is what dodot does with a file once a **rule** matches it. Rules are
pattern → handler mappings with a priority; checked highest-first, first match wins.

## Default rules (highest priority first)

| Prio | Handler  | Matches (at pack root)                                  |
|------|----------|--------------------------------------------------------|
| 100  | ignore   | (empty by default)                                     |
| 50   | skip     | README, LICENSE, CHANGELOG, CONTRIBUTING, AUTHORS, NOTICE, COPYING (case-insensitive) |
| 20   | install  | `install.sh`, `install.bash`, `install.zsh`            |
| 10   | homebrew | `Brewfile`                                             |
| 10   | nix      | `packages.nix`                                         |
| 10   | path     | `bin/`                                                 |
| 10   | shell    | `*.sh`, `*.bash`, `*.zsh`                              |
| 0    | symlink  | catch-all — anything not claimed above                 |

Override dispatch per-pack or repo-wide in `.dodot.toml` under `[mappings]`
(e.g. `shell = ["aliases.sh"]`, `ignore = ["scratch.txt"]`).

## Deploy handlers

These produce filesystem work. Each entry notes its **liveness** — what propagates
on its own vs what needs another `dodot up`.

### symlink (catch-all)

Links the source file to its live target.

- **Default target:** `~/.config/<pack>/<file>` for XDG-style config, else `~/.<file>`.
- **Routing prefixes** (override the target, applied at pack root):
  - File-level (skip the pack namespace): `home.X` → `~/.X` · `xdg.X` →
    `$XDG_CONFIG_HOME/X` · `app.X` → app-support dir · `lib.X` → `~/Library/X` (macOS).
  - Directory-level (route a whole subtree): `_home/<rest>` → `~/.<rest>` ·
    `_xdg/<rest>` → `$XDG_CONFIG_HOME/<rest>` · `_app/<rest>` → app-support ·
    `_lib/<rest>` → `~/Library/<rest>` (macOS).
- **Liveness:** edits to the source are live immediately (live path *is* the source
  via the link). File-watching editors reload at once; startup-only programs
  (window managers, daemons, X resources) need their own reload. **Adding or
  removing a source file needs another `dodot up`.**

### shell

Arranges for a `*.sh`/`*.bash`/`*.zsh` file to be sourced at login.

- **Liveness:** editing the script is live for the *next* shell session — no second
  `up` needed; re-source or open a new shell. **Adding or removing a script needs
  another `dodot up`** (staging registers it for new shells).

### path

Puts a `bin/` directory on `$PATH`.

- **Liveness:** new executables dropped into the directory are immediately runnable
  in shells that already have it on `$PATH` (the directory is staged, not each
  file). New files need the execute bit (`auto_chmod_exec = true` sets it on the
  next `up`). **Adding a new pack with `bin/`, or removing one, needs another `up`.**

### install / homebrew / nix (provisioning)

One-shot setup, tracked by a sentinel so it doesn't re-run:

- **install** — runs `install.sh` once.
- **homebrew** — runs `brew bundle` on a `Brewfile`.
- **nix** — runs `nix profile install` on a `packages.nix`.
- **Liveness:** editing the script does **not** auto-rerun (conservative — it could
  be destructive). `dodot status` reports `never run` / `installed` / `older
  version (N lines ±)`; `dodot status --diff` shows the change. Apply edits with
  `dodot up --provision-rerun`. Skip provisioning entirely with `dodot up --no-provision`.

## Filter handlers

These drop a match *without* deploying it.

- **ignore** — silent drop, like `.gitignore`. Empty by default; add globs via
  `[mappings] ignore`.
- **skip** — drops but surfaces as `skipped` in `dodot status` (so you can see it
  was deliberately not deployed). Defaults cover README/LICENSE/etc.
- **gate** — drops on hosts where a predicate doesn't match. Driven by filename
  suffix `._<label>` (e.g. `install._darwin.sh`, `home.bashrc._linux`) or a
  `_<label>/` directory at pack root. Built-in labels: `darwin`, `linux`, `macos`,
  `arm64`, `aarch64`, `x86_64`; define more under `[gates]`.

## Not the same as `.dodotignore`

`.dodotignore` is a **zero-byte marker inside a directory** that stops the whole
directory from being discovered as a pack. The `ignore`/`skip` handlers act on
*individual files within* a pack. Don't conflate them.

## Extensions that preprocess before the handler runs

Applied before dispatch; the extension is stripped and the result flows to the
normal handler:

- `.tmpl` / `.template` — rendered as a Jinja2 template at deploy time.
- `.age` / `.gpg` — decrypted at deploy time to a 0600 file in the datastore, then
  symlinked.

For both, the source is **not** the deployed bytes — a different editing model. See
the **dodot-templates** skill.
