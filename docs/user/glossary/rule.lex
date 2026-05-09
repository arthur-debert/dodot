:: verified ::
Rule:
    A pattern-to-handler mapping with a priority. Rules are how dodot decides which handler claims which file. The defaults that ship with dodot:

    - `bin/` → path
    - `install.{sh,bash,zsh}` → install
    - `Brewfile` → homebrew
    - `*.{sh,bash,zsh}` at pack root → shell
    - `README`, `LICENSE`, `CHANGELOG`, `CONTRIBUTING`, … → skip
    - `*` → symlink (catch-all for anything no precise rule claimed)

    Rules are checked in descending priority order; first match wins, so more specific rules sit above broader ones. Override per-pack via `[mappings]` in `.dodot.toml` when a name doesn't match the convention you'd rather use.
