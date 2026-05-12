:: verified ::
The external handler

Deploys content that isn't authored by you but comes from upstream and should be refreshed over time — shell frameworks, plugin-manager bootstraps, community theme repos, single shared snippets. Declared per-pack via an `externals.toml` file at the pack root; each section declares one external resource.

1. The trigger file

    A pack opts in by placing `externals.toml` at its root. Each `[section]` is one external entry. The section name is the entry name.

    Minimal pack:

        my-shared/
            externals.toml

    A pack can also ship regular dotfiles alongside the externals — they go through the symlink / shell / install handlers as usual.

2. Entry types

    Four `type` values are recognized:

    - `file` — one file from an HTTP(S) URL.
    - `git-repo` — a shallow git clone.
    - `archive` — a downloaded archive, extracted whole.
    - `archive-file` — one named member extracted from an archive.

    Every entry needs a `target` — the path where the deployed copy should appear. `~/...` is the typical form and `~` expands to `$HOME`, but absolute paths (`/etc/foo`, `/usr/local/share/...`) are accepted too. The deployed copy is always a symlink into the datastore; the source of truth lives in `externals.toml` and the upstream URL.

3. type = "file"

    Fetches one file from an HTTP(S) URL, verifies its sha256, and symlinks the configured target at it.

    Example:

        [shared-aliases]
        type   = "file"
        url    = "https://example.com/aliases.sh"
        target = "~/.config/shared/aliases.sh"
        sha256 = "abc123..."

    :: toml ::

    `sha256` is mandatory — an unpinned remote file has no integrity story. Bumping the value in the TOML invalidates the sentinel and forces a re-fetch.

    Supported URL schemes: `http://`, `https://`, and `file://` (handy for local mirrors and tests).

4. type = "git-repo"

    A shallow clone (`git clone --depth=1 --filter=blob:none`) into the datastore. Per-run freshness is upstream-driven: each `dodot up` runs `git ls-remote` to see whether the remote SHA has moved, and only re-fetches when it has.

    Minimal entry:

        [oh-my-zsh]
        type   = "git-repo"
        url    = "https://github.com/ohmyzsh/ohmyzsh.git"
        target = "~/.oh-my-zsh"

    :: toml ::

    Three optional fields tune the behavior:

    `subpath`:
        Sparse-checkout cone-mode pattern. Only that subtree is materialized on disk, and the target symlink points at it (not the whole clone).

            [p10k]
            type    = "git-repo"
            url     = "https://github.com/romkatv/powerlevel10k.git"
            target  = "~/.config/zsh/themes/p10k"
            subpath = "themes"

        :: toml ::

    `ref`:
        Tracking reference (tag or branch). `ls-remote` polls this reference instead of `HEAD`; refresh fires when its SHA changes.

            [omz-stable]
            type   = "git-repo"
            url    = "https://github.com/ohmyzsh/ohmyzsh.git"
            target = "~/.oh-my-zsh"
            ref    = "v1.0.0"

        :: toml ::

    `commit`:
        Frozen commit SHA. Skips `ls-remote` entirely — the local clone is compared against the configured commit and only refreshes when the user edits the TOML.

            [tpm]
            type   = "git-repo"
            url    = "https://github.com/tmux-plugins/tpm.git"
            target = "~/.tmux/plugins/tpm"
            commit = "3a8b3f4a5b8d1c2e3f4a5b6c7d8e9f0a1b2c3d4e"

        :: toml ::

    `ref` and `commit` are mutually exclusive — setting both is a hard error at parse time.

    `git` must be on `$PATH`. A missing `git` is a hard failure (the user has to fix it); other git errors (`ls-remote` flakes, fetch failures) are treated as transient and leave the cached clone in place.

5. type = "archive"

    Downloads an archive, sha256-verifies, and extracts the whole tree into the datastore. The target symlink points at the extracted directory.

    Example:

        [theme-pack]
        type   = "archive"
        url    = "https://github.com/foo/themes/archive/refs/tags/v1.0.tar.gz"
        target = "~/.config/themes"
        sha256 = "deadbeef..."

    :: toml ::

    Supported formats: gzipped tar (`.tar.gz`, `.tgz`) and zip (`.zip`). Format is inferred from the URL filename — anything else needs an explicit `format = "tar-gz"` or `format = "zip"`.

    Archive entries with unsafe paths (absolute paths, `..` components) are rejected at extraction time.

6. type = "archive-file"

    Same fetch+verify pipeline as `archive`, but only one named member is extracted. The target symlink points at the single deployed file.

    Example:

        [setup-script]
        type   = "archive-file"
        url    = "https://github.com/foo/bar/archive/refs/heads/main.tar.gz"
        target = "~/.local/bin/foo-setup"
        sha256 = "feedface..."
        member = "bar-main/scripts/setup.sh"

    :: toml ::

    Useful when an archive ships many files but you only want one deployed.

7. Datastore layout

    All externals land under one per-pack directory inside dodot's XDG data dir:

        $XDG_DATA_HOME/dodot/packs/<pack>/external/
            <entry-name>/         # the fetched content
            <entry-name>-...      # sentinel files

    :: text ::

    `$XDG_DATA_HOME` defaults to `~/.local/share`, so on a stock setup the path resolves to `~/.local/share/dodot/packs/<pack>/external/`. If you've set `XDG_DATA_HOME` to something else, that takes precedence.

    For a `file` entry, `<entry-name>/<basename>` is the single fetched file. For a `git-repo` entry, `<entry-name>/` is the cloned tree. For `archive`, `<entry-name>/` is the extracted tree. For `archive-file`, `<entry-name>/<basename-of-member>` is the single extracted file.

    The deployed target is a symlink into this tree.

8. Status output

    `dodot status` shows one coarse row per pack that has an `externals.toml`: deployed or pending. Per-entry detail — which entries have drifted, which couldn't be checked, which need a refresh — surfaces through `--check-drift` (next section) and through the sentinel filenames in the datastore, which encode the content signature:

    - `file`: `<entry-name>-<sha-prefix>`
    - `git-repo`: `<entry-name>-git-<sha-prefix>`
    - `archive`: `<entry-name>-archive-<sha-prefix>`
    - `archive-file`: `<entry-name>-archive-<sha-prefix>-<member-hash>`

    `dodot probe show-data-dir` (or just listing the datastore directly) is the fastest way to read those off if you need to confirm which version is live for a specific entry.

9. Drift detection (--check-drift)

    Pass `dodot status --check-drift` to ask "did the user edit the deployed copy?" — a different question than upstream-freshness, which fires automatically on every `up`.

    Per-type behaviour:

    - `file` — sha256 of the deployed copy vs the configured sha256. Drift = bytes differ.
    - `git-repo` — `git status --porcelain` on the local clone. Drift = non-empty output.
    - `archive` / `archive-file` — surfaced as "not implemented" for now; silence would suggest "clean" and is misleading.

    Drift is opt-in because hashing every deployed external on every `status` invocation is wrong for big trees like `oh-my-zsh`. Edits at the deployed location will be clobbered on the next upstream refresh — externals are upstream-tracking, not authorship targets.

10. Failure posture

    Failures fall into two categories:

    - *Integrity failure* (sha256 mismatch on `file` / `archive`) is fatal. The bytes are refused and nothing is written.
    - *Network failure* is soft: the cached copy stays in place, the failure is surfaced as a non-success result, and other intents in the pack still run. `dodot up` on a plane will keep deploying everything else.

11. Live edits

    The deployed copy is a symlink into the datastore — so any "edit" you make at `~/.oh-my-zsh/...` lands on the datastore file, which the handler will overwrite on the next refresh.

    What auto-propagates:
        Nothing on the source side, because there is no source on your machine. The TOML is the source.

    What needs another `dodot up`:
        Editing `externals.toml` itself — adding entries, bumping `sha256`, changing `ref` / `commit` / `subpath`, switching `target`. The next `up` re-evaluates the file and refreshes anything that moved.

    Program-reload caveats:
        Shell frameworks like oh-my-zsh need a new shell session to pick up the deployed content (or `source` the relevant file). dodot can put the symlink in place but can't reload your running shell for you.

    The handler will *not* preserve your edits at the deployed location. If you want to customize a fetched file, copy it into your dotfiles repo and let the symlink handler deploy it from there — externals are for tracking upstream, not for editing.

12. Configuration

    Under `[mappings]` (root or per-pack `.dodot.toml`):

        [mappings]
        externals = ["externals.toml"]

    :: toml ::

    Default is `["externals.toml"]`. Override the filename if you have a strong reason; the default reads cleanly outside dodot context ("list of externals this pack pulls in") so renaming is rarely worth it.
