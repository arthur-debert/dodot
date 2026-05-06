:: verified ::
Pack:
    A directory under your dotfiles root whose contents belong together — `vim/`, `git/`, `work/`, whatever you'd reach for as one unit. Packs are what `dodot up` and `dodot down` act on. The organizing criterion is yours; by app, by role, by machine — dodot doesn't impose one. Every top-level directory in your dotfiles root is a pack unless it contains a `.dodotignore` file.

    While the criterion is open, for symlinks that go under `$XDG_CONFIG_HOME` (typically `~/.config/<app>/`) the pack name is used, by default, as the directory name under `~/.config/` — e.g. an `nvim/` pack symlinks to `~/.config/nvim/`. So for app-config-bearing packs, it's common to name the pack after the app. For files that don't go under `~/.config/`, the pack name is less important, and you can organize them as you see fit.
