:: verified ::
dodot refresh

The "make `git` see deployed-side template edits" command. When you edit a deployed file produced by a template (the rendered output, not the `.tmpl` source), git wouldn't normally notice — its stat-cache says the source mtime hasn't changed, so it doesn't re-read. `dodot refresh` walks every cached preprocessor baseline, hashes each deployed file, and copies the deployed file's mtime onto the source whenever the bytes have diverged. Next `git status` re-reads the source and the change surfaces normally.

You usually don't run this by hand. The intended pattern is to wrap `git` in a shell alias that runs `dodot refresh --quiet` first, so `git status` and `git diff` always see current reality. See [./git-augmentation.lex] for installing that wrapper.

1. When you reach for it

    - You've installed templates in a pack, edited the deployed file in place (because that's where you read it from), and want to commit. `dodot refresh` is the bridge between deployed-side edits and git.
    - You're driving an editor or file-watcher integration that wants to know which template sources are dirty: `dodot refresh --list-paths`.
    - You're debugging "git doesn't show my edit" — running `dodot refresh` by hand confirms or rules out the source/deployed mtime drift.

2. What it does

    Walks the per-file baseline cache (one entry per template-rendered file). For each:

    - Read the deployed bytes from `<data_dir>/packs/<pack>/<handler>/<filename>`.
    - Hash them.
    - Compare to the baseline's recorded `rendered_hash`.
    - If they differ, copy the deployed file's mtime onto the *source* file so git's stat-cache invalidates and `git status` re-reads on next invocation.

    The baseline cache is what makes this work — it's what dodot wrote during the last `dodot up` to record "the deployed bytes for this template were this hash at this mtime." Without templates in your repo, refresh is a no-op.

3. Modes

        | Mode             | Effect                                                                                    |
        | (default)        | Touch source mtimes; print a short report of touched / clean / missing entries.          |
        | `--quiet`        | Touch source mtimes silently. Intended for shell aliases that wrap `git`.                |
        | `--list-paths`   | Print the source paths that would be touched, one per line. Does *not* write mtimes.     |

    :: table align=ll ::

    `--list-paths` exists for editor / file-watcher integrations that want to drive the touch themselves rather than have dodot write mtimes. The two are mutually exclusive (`--quiet --list-paths` errors).

4. Reported per-file actions

    Default and `--list-paths` modes group entries by what was found:

    - *Clean.* Deployed hash matches baseline; nothing to do.
    - *Touched.* Deployed bytes diverged; mtime copied (or, in `--list-paths`, listed).
    - *Missing deployed.* Datastore-side file is gone (e.g. deleted by hand). Reported but not actioned — `dodot up` re-creates it.
    - *Missing source.* Cached source path no longer exists on disk. Reported.

5. Examples

        # The intended usage — wrapped via shell alias
        alias git='dodot refresh --quiet && command git'
        # see also: dodot git-install-alias

        # One-off: refresh and see what was touched
        dodot refresh

        # Editor integration: just print the paths that need touching
        dodot refresh --list-paths

        # Debug: confirm git would now see your edit
        dodot refresh
        git status

    :: shell ::

6. Watch out for

    - *No-op without templates.* If your repo doesn't use any preprocessor (templates, secrets-injection, plists, …), there's nothing in the baseline cache, and `refresh` reports zero entries every time. You don't need it.
    - *The wrapper alias is the canonical entry point.* The Tier-2 install (`dodot git-install-alias`) writes that alias into your rc. Running `dodot refresh` by hand is fine for inspection but tedious as a workflow — install the alias and move on.
    - *Don't run `refresh` from a directory unrelated to your dotfiles.* It looks up dodot's data dir from your usual env / cwd resolution; running it from inside a different repo tree just to "be safe" doesn't help and may produce confusing "no baselines" output.
    - *`refresh` only handles forward direction.* It tells git that the source is dirty so the deployed-side change surfaces. Reverse-merging the deployed-side edit back into the template *source* (so the source TOML/JSON/etc. matches what you actually edited) is `dodot transform check`. That's a separate command and a separate workflow — see [./git-augmentation.lex].
