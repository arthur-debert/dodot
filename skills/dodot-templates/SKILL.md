---
name: dodot-templates
description: Author and sync dodot templates and secrets — render config that varies per host/OS, inject secret values or encrypt whole files, and reverse-merge deployed-side edits back to source. Use when the user wants a dotfile to vary between machines, references Jinja/{{ }} in dotfiles, wants to keep a token/password out of git, encrypts a file with age/gpg, edits a rendered/deployed file and needs it back in source, or sets up the dodot git pre-commit hook for templates.
---

# dodot templates & secrets

dodot **preprocesses** some pack files at deploy time: `.tmpl`/`.template` files are
rendered as Jinja2, and secrets are resolved from your password manager or decrypted
from `.age`/`.gpg`. This skill is for authoring that content and keeping it in sync.

Prerequisite mental model: packs, handlers, `up`/`down`/`status`, and the
datastore — see the **using-dodot** skill. This skill assumes it.

## The one inversion that matters: source ≠ live

For an ordinary symlinked file, the source and the live (deployed) file are the
*same bytes*. **Preprocessing breaks that.** The source is a `.tmpl`/`.age`/`.gpg`
file; the live file is the *rendered* / *decrypted* output, written to the
datastore (`~/.local/share/dodot/packs/<pack>/preprocessed/<name>`) and symlinked
into place. They are **different bytes**, and that drives everything below.

Two consequences you must hold:

- **Rendering is forward-only and automatic.** Every `dodot up` re-renders from
  source. Edit the template or change a variable → it picks up on the next `up`.
  There is no separate render step.
- **Editing the *live* file does not flow back to source on its own.** If you edit
  the rendered `~/.gitconfig` directly, the `.tmpl` source still has the old
  content, and the next `up` overwrites your edit. Getting it back to source is the
  reverse-sync (below).

## The rule for an agent: edit the source, not the output

When changing a templated file, **find and edit the `.tmpl` source**, then
`dodot up` and verify the rendered output. This sidesteps reverse-sync entirely.

Only reach for the reverse-sync when the user has *already* edited the deployed
file in place, or explicitly wants the git-augmentation installed.

## Reverse-sync: getting deployed-side edits back to source

Two separate problems, two separate commands. Don't conflate them:

1. **git can't see the edit.** git's stat-cache says the source mtime is unchanged,
   so `git status` shows nothing. `dodot refresh` walks the baseline cache, finds
   deployed files whose bytes diverged, and touches the *source* mtime so git
   re-reads. It does **not** change source content — only visibility.
2. **The source content is still stale.** `dodot transform check` actually
   reverse-merges the deployed bytes back into the `.tmpl` source on disk. Where it
   can't merge cleanly it inserts `dodot-conflict` markers for you to resolve.
   `--strict` makes it exit 1 while markers remain (how the pre-commit hook blocks
   bad commits). **It writes to your source files** — use `--dry-run` to preview.

`dodot transform status` is the read-only map: `synced` / `output_changed` (deployed
edited) / `input_changed` (source edited) / `both` / `missing`.

The canonical setup wires both into git so this is automatic:

```bash
dodot git-install-alias        # git wrapper runs `dodot refresh --quiet` first → git sees deployed edits
dodot transform install-hook   # pre-commit runs refresh + `transform check --strict`
```

## Workflows

### Add a templated file (vary content per host)

```bash
# 1. Author the source with the .tmpl extension, referencing variables in {{ }}:
#      git/gitconfig.tmpl   ->   name = {{ name }}
# 2. Define the variable(s) in .dodot.toml (root for all packs, or pack-local):
#      [preprocessor.template.vars]
#      name = "Alice"
dodot up git
cat ~/.config/git/gitconfig            # verify rendered output (status shows the *stripped* name)
```

Undefined variables are a hard error — `up` refuses the pack. Mark genuinely
optional values with Jinja's `default`: `{{ env.EDITOR | default("nvim") }}`.
Built-in namespaces: `dodot.*` (os, arch, hostname, …), `env.*`, and your bare vars.
See `TEMPLATES.md`.

### Template vs gate — pick the right tool

A template is for when the file *always deploys* but its **content** varies. When
the question is binary — *does this file deploy on this host at all?* — use a
**gate** (`Brewfile._darwin`, `install._darwin.sh`, `[pack] os = ["darwin"]`), not
a whole file wrapped in `{% if dodot.os %}`. They compose:
`aliases._darwin.sh.tmpl` gates first, renders second.

### Inject a secret value (keep a token out of git)

```bash
dodot secret list                      # inventory which providers the repo references
# enable in .dodot.toml: [secret] enabled = true  +  [secret.providers.<scheme>] enabled = true
# reference in a template:  token = "{{ secret('pass:dodot/api_token') }}"
dodot secret probe                     # confirm the provider authenticates
dodot up <pack>                        # source keeps {{ secret(...) }}; deployed gets the real value
```

Six providers (`pass`, `op`, `bw`, `sops`, `keychain`, `secret-tool`), each with its
own reference syntax — see `SECRETS.md`. Multi-line secrets are refused here; use
whole-file instead.

### Encrypt a whole file

Drop a `.age` or `.gpg` file in the pack, enable `[preprocessor.age]` /
`[preprocessor.gpg]`; `dodot up` decrypts to a 0600 datastore file and symlinks it.
There is **no `dodot secret edit`** — the edit loop is decrypt → edit → re-encrypt →
commit by hand. See `SECRETS.md`.

### Sync a deployed-side edit back to source

```bash
dodot transform status                 # confirm the file shows output-changed / both
dodot transform check --dry-run        # preview the reverse-merge
dodot transform check                  # write it back into the .tmpl source
# resolve any dodot-conflict markers in the source, then commit
```

## Going deeper

- **`TEMPLATES.md`** — the three namespaces and built-ins, strict-mode errors and
  `default`, custom extensions, collision rules, where rendered output lives.
- **`SECRETS.md`** — the six providers with reference syntax and quirks, value
  injection vs whole-file, the manual edit loop, `secret probe`/`list`,
  troubleshooting.

Per-command help is authoritative: `dodot transform --help`, `dodot refresh --help`,
`dodot secret --help`.
