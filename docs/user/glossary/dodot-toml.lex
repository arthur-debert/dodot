:: verified ::
`.dodot.toml`:
    Optional configuration for a pack — or, at the dotfiles root, defaults for every pack. Most setups need none of it.

    Reach for it when the conventions don't match your file names (`[mappings]`), when symlinks should land somewhere other than the default (`[symlink.targets]`), when a pack should only deploy on certain hosts (`[pack] os`, `[gates]`), or when a preprocessor needs tuning.

    :: note :: `[pack] os` is valid only inside a pack's `.dodot.toml`, never at the root — root config can't pin every pack to one OS.

    Pack-level config wins over root-level config for that pack. Files are key-sparse: only the keys you set are applied; everything else inherits from the root config or the built-in defaults.

    The starting point is a commented sample:

        $ dodot config gen > .dodot.toml

    :: shell ::

    The comments should be enough to clarify usage — uncomment the keys you care about and go.
