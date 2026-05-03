# Handlers

A handler is the thing that decides what to do with a file in your pack. Each handler has exactly one job: link configs, source shell scripts, add directories to `$PATH`, run install scripts once, install Brewfiles, or filter files out of dispatch entirely. dodot ships with seven handlers, and most users never need to think about them — the defaults match common dotfile conventions.

This document is your reference for what each handler claims by default, what it does, and how to configure it. For the conceptual overview (matching model, execution order, why handlers look the way they do), see [the reference](../reference/handlers.lex).

## Defaults at a Glance

These are the file patterns each handler claims by default. Anything not matched here flows to `symlink`.

| Handler  | Claims by default                                                                            | What happens                                            |
| -------- | -------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| ignore   | (empty by default)                                                                           | Drop silently — same contract as `.gitignore`           |
| skip     | `README`/`README.*`, `LICENSE`/`LICENSE.*`, `CHANGELOG`/`CHANGELOG.*`, etc. (case-insensitive) | List in `dodot status` as `skipped`; do not deploy      |
| homebrew | `Brewfile`                                                                                   | `brew bundle` once per content hash                     |
| install  | `install.sh`, `install.bash`, `install.zsh`                                                  | Script runs once per content hash                       |
| path     | `bin/` (directory)                                                                           | Directory prepended to `$PATH`                          |
| shell    | `aliases.{sh,bash,zsh}`, `profile.{sh,bash,zsh}`, `login.{sh,bash,zsh}`, `env.{sh,bash,zsh}` | File sourced at shell login                             |
| symlink  | Anything else (catchall)                                                                     | File or directory linked to `~/.config/<pack>/` or `~/` |

Override any of these in `.dodot.toml` under `[mappings]`. Handler patterns are fully replaceable; you cannot, however, add a brand-new handler from config — the handler list itself is fixed.

Default `[mappings]`:

```toml
[mappings]
path     = "bin"
install  = ["install.sh", "install.bash", "install.zsh"]
shell    = [
    "aliases.sh", "aliases.bash", "aliases.zsh",
    "profile.sh", "profile.bash", "profile.zsh",
    "login.sh",   "login.bash",   "login.zsh",
    "env.sh",     "env.bash",     "env.zsh",
]
homebrew = "Brewfile"
ignore   = []
skip     = [
    "README", "README.*",
    "LICENSE", "LICENSE.*",
    "CHANGELOG", "CHANGELOG.*",
    "CONTRIBUTING", "CONTRIBUTING.*",
    "AUTHORS", "AUTHORS.*",
    "NOTICE", "NOTICE.*",
    "COPYING", "COPYING.*",
]
```

Run `dodot config gen -o .dodot.toml` to write a fully-commented starter.

## Execution Order

Within a single pack, handlers run in this fixed order:

1. **ignore / skip** (filter phase) — drop matched files before any deploying handler can claim them.
2. **homebrew** — install packages first, so anything later can depend on them.
3. **install** — run user setup scripts after `brew` is available.
4. **path** — stage `bin/` onto `$PATH` before shell init reads it.
5. **shell** — source shell startup files, which may reference binaries from `path`.
6. **symlink** — catchall, runs last so precise handlers claim their files first.

Across packs, dodot processes packs in lexicographic order of their on-disk directory names. For the small handful of cases where pack ordering matters (Homebrew shellenv before anything that calls `brew`, `compinit` after completion plugins), name your directories with a numeric prefix: `010-brew`, `100-zsh`, `900-starship`. The prefix is invisible to user-facing surfaces — `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua`, not `~/.config/010-nvim/`.

## The Seven Handlers

### symlink

Creates a symlink from a deployed location back to a file or directory in your pack. This is the default for any file that no other handler claims.

**Default deploy path:** every pack-root entry — file or directory — defaults to `$XDG_CONFIG_HOME/<pack>/<name>`. So `nvim/init.lua` → `~/.config/nvim/init.lua`, `warp/themes/` → `~/.config/warp/themes/`. The pack name namespaces config under XDG, matching how modern tools (nvim, helix, ghostty, kitty, alacritty, …) actually read their configuration.

**Escape hatches** for the cases where the XDG default is wrong:

- `home.<file>` prefix on a top-level file → `$HOME/.<file>`. So `git/home.gitconfig` → `~/.gitconfig`. Top-level files only; nested `home.X` is treated literally.
- `_home/<rest>` directory → `$HOME/.<rest>` (raw, no pack namespace). Useful when a pack groups files that all belong in `$HOME`.
- `_xdg/<rest>` directory → `$XDG_CONFIG_HOME/<rest>` (raw, no pack namespace). Useful when a pack name doesn't match the target program (a `term-config` pack with `_xdg/ghostty/config`).
- `[symlink] force_home` config list → routes legacy-shell-and-credential paths to `$HOME` regardless of XDG.
- `[symlink.targets]` config map → fully custom paths.

For the full path-resolution rules with examples, see [Symlink Deployment Paths](../reference/symlink-paths.lex).

**Configurability** under `[symlink]` in `.dodot.toml`:

```toml
[symlink]

# Files/dirs that must land in $HOME instead of $XDG_CONFIG_HOME.
# Matched against the first path segment, leading dot ignored.
force_home = [
    "ssh",            # .ssh/ - security critical
    "aws",            # .aws/ - credentials
    "kube",           # .kube/ - kubernetes config
    "bashrc",         # .bashrc - shell expects in $HOME
    "zshrc",          # .zshrc
    "profile",        # .profile
    "bash_profile",
    "bash_login",
    "bash_logout",
    "inputrc",        # readline config
]

# Files dodot refuses to symlink — almost always a mistake to deploy these.
# Override to remove an entry and allow it.
protected_paths = [
    ".ssh/id_rsa",
    ".ssh/id_ed25519",
    ".ssh/id_dsa",
    ".ssh/id_ecdsa",
    ".ssh/authorized_keys",
    ".gnupg",
    ".aws/credentials",
    ".password-store",
    ".config/gh/hosts.yml",
    ".kube/config",
    ".docker/config.json",
]

# Per-file custom symlink targets.
# Absolute paths used as-is; relative paths resolved from $XDG_CONFIG_HOME.
[symlink.targets]
"mysterious.conf" = "/var/etc/mysterious.conf"
"home-bound.conf" = "my-documents/home-bound.conf"
```

**Per-file mode for directories.** By default, a top-level directory is wholesale-linked (one symlink for the whole directory). dodot drops into per-file mode for that directory if any file inside it appears in `protected_paths` or as a key in `[symlink.targets]`. Per-file mode emits one symlink per file, each resolved independently — protected files are skipped.

### shell

Sources shell scripts at login. Matched files are staged into the datastore; the generated `dodot-init.sh` (which you load with `eval "$(dodot init-sh)"`) emits a `source` line for each.

**Default claims:** `aliases.{sh,bash,zsh}`, `profile.{sh,bash,zsh}`, `login.{sh,bash,zsh}`, `env.{sh,bash,zsh}`.

**Extensions are load-bearing.** Sourced files run *in your interactive shell*, so `.zsh` files only parse cleanly in zsh sessions and `.bash` files in bash sessions. `.sh` is the portable bucket — use it for snippets that work in either. Most users only run one shell and never hit the mismatch; if you switch shells, split your config by extension.

**Configurability:**

```toml
[mappings]
shell = ["aliases.sh", "myextras.zsh", "work.bash"]
```

The shell handler has no `[shell]` section of its own — the mapping list *is* the configuration.

### path

Adds a directory to `$PATH`. The conventional match is a `bin/` directory inside a pack; its contents become directly executable from any shell. Like `shell`, this rides on `dodot-init.sh` — the datastore records which directories should be on PATH, and the init script prepends them.

**Default claim:** `bin/` (directory).

**Configurability:**

```toml
[mappings]
path = "scripts"   # rename the matched dir; trailing slash auto-added

[path]
# Auto-chmod +x on files inside path-handler directories. On by default.
# Useful because git on macOS defaults to core.fileMode=false, so cloned
# scripts may not have the execute bit. With this on, dodot ensures every
# file in a path-handler directory is executable on `dodot up`.
# Failures report as warnings, not hard errors.
# Set to false if you have non-executable files in `bin/` (data files,
# library scripts sourced by other scripts).
auto_chmod_exec = true
```

### install

Runs an arbitrary shell script once, tracked by a sentinel keyed on the script's content hash. Use this for machine-specific setup that the other handlers don't cover: language toolchains, window manager configuration, system defaults.

**Default claims:** `install.sh`, `install.bash`, `install.zsh`.

**Interpreter is picked by extension**, not by your login shell:

- `.sh`, `.bash`, or unknown extension → run with `bash`
- `.zsh` → run with `zsh`

The script runs in a fresh subprocess, so your interactive shell state (aliases, functions, options) is invisible to it regardless. The extension is the contract the pack author declares: `install.zsh` announces zsh-specific syntax; `install.sh` announces portability.

**Sentinels.** When an install script runs successfully, dodot writes `<filename>-<checksum>` (e.g. `install.sh-a1b2c3d4e5f6a7b8`) into the datastore. On subsequent `dodot up`, the script is skipped if its sentinel exists. Edit the script and the checksum changes, the sentinel name changes, and the script re-runs automatically. Override:

- `--no-provision` skips install and homebrew entirely for this run.
- `--provision-rerun` forces them to re-run even when sentinels exist. Use after changing an install script when the change is too subtle to alter the content (rare), or to rerun without an input change.

**Multiple matches.** A pack with both `install.sh` and `install.zsh` runs *all of them*, each tracked by its own sentinel. There is no "pick the best one" logic — if you want only one to run, ship only one.

**Output.** By default `dodot up` keeps install-script output quiet — only start/end markers and a couple of conventions are surfaced:

- **Header block.** When a script starts, the leading comment block (the contiguous `#`-prefixed lines after the optional shebang) is printed so you see what's about to run. Document your script the way you'd want a teammate to read it.
- **`# status:` markers.** Lines on stdout matching `# status: <message>` (or `#status: <message>`) are printed as live progress while the script runs. Sprinkle these at phase boundaries so a long-running script doesn't look hung:

  ```sh
  #!/bin/bash
  # Install nvm
  # Requires curl

  # status: downloading installer
  curl -sL https://example.com/install.sh -o /tmp/inst
  # status: running installer
  bash /tmp/inst
  ```

  The convention is tool-agnostic: the `# status:` lines are just shell comments when the script is run by hand outside dodot.

- **`--verbose`.** Pass `--verbose` (or `--debug`) to `dodot up` to also stream the script's raw stdout/stderr in real time — useful when debugging a misbehaving install. On failure, captured stderr is dumped automatically even without `--verbose`.

**Configurability:**

```toml
[mappings]
install = ["setup.sh", "bootstrap.zsh"]
```

`install` is list-only — even a single script must be written as `install = ["install.sh"]`. The single-string form does not parse.

The install handler has no dedicated `[install]` section.

### homebrew

Runs `brew bundle` against a `Brewfile`, once per content-hash. macOS-only in practice. Functionally a specialization of `install` with a more ergonomic default for its common case.

**Default claim:** `Brewfile`.

**Sentinel behavior** is identical to `install`: edit the Brewfile, the checksum changes, `brew bundle` runs again. `--no-provision` and `--provision-rerun` apply here too.

**Configurability:**

```toml
[mappings]
homebrew = "MyBrewfile"
```

Single-string only, unlike `install`. The homebrew handler has no dedicated section.

## Configuration vs Code Execution

Handlers fall into two categories:

- **Configuration handlers** — `symlink`, `shell`, `path`. Idempotent filesystem work. `dodot up` always runs them in full and wipes per-pack state before re-applying, so a deleted source file doesn't leave an orphan link behind.
- **Code execution handlers** — `install`, `homebrew`. Run user-authored shell commands that may not be idempotent. Tracked by sentinels, skipped on subsequent runs unless the input content changes. Use `--no-provision` to skip them entirely or `--provision-rerun` to force re-execution.

This split is why `--no-provision` exists. On a daily basis you want fast `dodot up` runs that re-link configuration without re-running multi-second `brew bundle` calls; on a fresh machine you want everything to run.

## Keeping Files Out of Handler Dispatch

### ignore (filter)

Claims matches and drops them silently — same contract as `.gitignore`. No entry in `dodot status`, no executable intent. Configured via `[mappings] ignore` (default empty). Useful for build artifacts, scratch files, anything you don't want dodot to know about.

```toml
[mappings]
ignore = ["*.bak", "scratch.txt"]
```

### skip (filter)

Claims matches, surfaces them in `dodot status` as `skipped`, but produces no executable intent — `dodot up` will not deploy them. Configured via `[mappings] skip`. The defaults cover the common documentation and legal files that packs ship alongside real config:

```
README, README.*, LICENSE, LICENSE.*, CHANGELOG, CHANGELOG.*,
CONTRIBUTING, CONTRIBUTING.*, AUTHORS, AUTHORS.*, NOTICE, NOTICE.*,
COPYING, COPYING.*
```

Matched case-insensitively against the basename. Override per-pack with `skip = []` to deploy a `README` intentionally, or replace the list to use your own conventions.

```toml
# pack-local: deploy our README, but skip TODO.md
[mappings]
skip = ["TODO.md"]
```

### Choosing between them

| You want…                                     | Use                            |
| --------------------------------------------- | ------------------------------ |
| The file invisible to dodot                   | `[mappings] ignore` (or `[pack] ignore`) |
| The file visible in `dodot status`, undeployed | `[mappings] skip`              |
| The whole pack ignored                        | `.dodotignore` marker file     |
| The file invisible during pack scanning       | `[pack] ignore`                |

`[pack] ignore` is the broadest hammer — its glob patterns are excluded from pack discovery and file scanning entirely, so matched files never become candidates for any handler. The defaults (`.git`, `node_modules`, `.DS_Store`, `*.swp`, …) cover version-control noise and editor swapfiles. `[mappings] ignore` and `[mappings] skip` operate one layer down: the file is discovered, but a filter handler claims it before any deploying handler sees it.

When both `ignore` and `skip` could match, `ignore` wins (it's higher priority) — a file the user said to drop is dropped, full stop.

To skip an *entire pack*, drop a `.dodotignore` marker file in that pack's directory.

See [Configuration](configuration.lex) for the full set of `.dodot.toml` keys.
