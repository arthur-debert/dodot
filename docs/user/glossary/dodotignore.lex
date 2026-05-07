:: verified ::
`.dodotignore`:
    A marker file inside a directory that tells dodot to skip the whole directory as a pack. Pure file-presence check — the contents are never read, the file is never opened.

    Useful for directories that live in your dotfiles repo but aren't meant to be deployed: scratch space, notes, README-only packs, work-in-progress.

    Distinct from `[mappings] ignore` and `[mappings] skip` in `.dodot.toml`, which drop individual files inside a pack that's otherwise active.
