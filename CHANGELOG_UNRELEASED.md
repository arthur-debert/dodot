# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use sections: Added, Changed, Deprecated, Removed, Fixed, Security.

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
