# Templates reference

Any pack file ending in `.tmpl` or `.template` is rendered as a Jinja2 template at
deploy time. The extension is stripped and the result flows to the normal handler
pipeline: `git/gitconfig.tmpl` renders and symlinks as `~/.gitconfig`. `dodot
status` shows the **stripped** name. Every `dodot up` re-renders — no `dodot render`
step.

## What you can reference

Three namespaces are always available:

- **`dodot.*`** — built-ins describing the machine:
  `dodot.os` (`"linux"`/`"macos"`), `dodot.arch` (`"aarch64"`/`"x86_64"`),
  `dodot.hostname`, `dodot.username`, `dodot.home`, `dodot.dotfiles_root`.
  `os`/`arch`/`home`/`dotfiles_root` are always set; `hostname`/`username` are
  best-effort (the key is *omitted* if undetectable, not blanked).
- **`env.*`** — process environment at render time. `{{ env.EDITOR }}` reads
  `$EDITOR` as `dodot up` runs.
- **bare names** — your own vars from `[preprocessor.template.vars]`.

## Defining variables

```toml
# .dodot.toml — root (all packs) or pack (that pack only)
[preprocessor.template.vars]
editor = "nvim"
host_tier = "workstation"
```

Values are strings, referenced by bare name (`{{ editor }}`). Pack-level vars
override root-level of the same name (replace, no merge). `dodot` and `env` are
reserved — using them as var names is a startup error.

## Strict mode — undefined is an error

Referencing a variable that doesn't exist is a render error and `up` refuses to
deploy that pack (deliberate: no silent empty-string substitution). Mark genuinely
optional values with Jinja's `default` filter — works across all three namespaces:

```jinja
editor = {{ env.EDITOR | default("nvim") }}
host   = {{ dodot.hostname | default("unknown") }}
tier   = {{ host_tier | default("unspecified") }}
```

## Branching

Standard Jinja `{% if %}` / `{% else %}` / `{% endif %}`:

```jinja
{% if dodot.os == "macos" %}
export HOMEBREW_PREFIX=/opt/homebrew
{% else %}
export HOMEBREW_PREFIX=/home/linuxbrew/.linuxbrew
{% endif %}
```

For anything past OS/hostname, define a classifier var (`host_role = "work"`) and
branch on it. For *deploy-or-not* questions, prefer a gate over a whole-file `{% if
%}` — see the main skill's "template vs gate" note.

## Disabling preprocessing

```toml
[preprocessor]
enabled = false        # .tmpl files deploy verbatim, extension intact
```

The same key in a pack's `.dodot.toml` overrides the root — enable globally, disable
for one pack.

## Custom extensions

```toml
[preprocessor.template]
extensions = ["j2", "jinja"]    # leading dot optional; longest match wins on overlap
```

## Collisions

A pack can't hold both `config.toml` and `config.toml.tmpl` — they'd deploy to the
same name. dodot refuses the pack with a collision error rather than picking
silently. Same if two preprocessors produce the same output name.

## Where rendered output lives

`$XDG_DATA_HOME/dodot/packs/<pack>/preprocessed/<stripped-name>` (directory
structure preserved). That file is what the handlers see and what `refresh` /
`transform` hash. The install handler's completion sentinel hashes the **rendered**
script, so changing a variable (or moving machines, changing `dodot.hostname`)
re-triggers the install step even when the `.tmpl` source is unchanged.
