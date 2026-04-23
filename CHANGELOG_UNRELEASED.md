# Unreleased Changes

Add entries here as changes are made. On release, copy this content into
CHANGELOG.md under a new version heading and clear this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Use sections: Added, Changed, Deprecated, Removed, Fixed, Security.

### Changed

- **BREAKING:** Symlink handler now deploys every pack-root entry — file or directory — to `$XDG_CONFIG_HOME/<pack>/<name>` by default (#48). Previously, top-level files defaulted to `$HOME/.<name>` and top-level directories to `$XDG_CONFIG_HOME/<name>` (no pack namespace). The new rule is consistent across files and dirs and matches modern tool conventions (nvim, helix, ghostty, kitty, alacritty, lazygit, starship, …) without forcing users to write `pack/program/` doubled paths.
- **BREAKING:** Per-file `$HOME` opt-in convention renamed: `dot.X` → `home.X`. The semantic is unchanged (`<pack>/home.bashrc` → `~/.bashrc`); the new name reads as "deploy to home as .X" instead of "this filename has a literal dot." All `[symlink.targets]`, `_home/`, `_xdg/`, `force_home`, and `protected_paths` overrides keep their existing semantics.
- The `_home/` and `_xdg/` directory prefixes are now always per-file (never wholesale-linked at the top level) — wholesale-linking the prefix dir itself would have baked the literal `_home`/`_xdg` segment into the deploy path, which is never what users meant.

### Migration notes (#48)

- A pack with `git/gitconfig` previously deployed to `~/.gitconfig`. It now deploys to `~/.config/git/gitconfig` (which git itself reads via XDG since 2.20). To keep the legacy `$HOME` path, rename the file to `git/home.gitconfig` (per-file home opt-in) or add a `[symlink.targets]` override.
- A pack with `warp/themes/` previously deployed to `~/.config/themes`. It now deploys to `~/.config/warp/themes`. Pin consumers to the new path or use `_xdg/themes/` inside the pack to skip the namespace.
- A pack with `git/dot.gitconfig` (old per-file convention) needs to be renamed to `git/home.gitconfig` — `dot.X` is no longer recognized.
- No change for files matching `force_home` (ssh, gpg, bashrc, zshrc, profile, inputrc, etc.) — those still deploy to `$HOME/.<name>`.
- No change for files routed via `[symlink.targets]`, `_home/`, or `_xdg/` directory prefixes — those keep their existing behavior.
