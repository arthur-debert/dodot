:: verified ::
dodot template

The "template-source git integration" command family. Two subcommands:

- `dodot template clean` — the git clean filter for template sources. Invoked by git, not by you.
- `dodot template install-filter` — register the dodot-template clean filter in the dotfiles repo's `.git/config`.

This is the template-side rung of the git-augmentation set. See [./git-augmentation.lex] for the full landscape, including the pre-commit hook and the plist filters.

1. template install-filter

    Writes the `[filter "dodot-template"]` block to your dotfiles repo's `.git/config`:

    - `clean = dodot template clean --path %f`
    - `smudge = cat`
    - `required = true`

    Per-clone, per-machine — `.git/config` doesn't travel with the repo. Run once per machine after cloning, or let the install ladder do it.

    Pair with a committed `.gitattributes` line:

        *.tmpl filter=dodot-template

    :: text ::

    Idempotent. Re-running on an already-installed filter is a no-op success.

    Examples:

        dodot template install-filter      # write the .git/config block

    :: shell ::

2. template clean (the filter binary)

    The clean filter that git invokes when reading a template source file. You don't run this by hand in normal use — git invokes it via the `[filter "dodot-template"]` block.

    Mechanics: when git reads a template source file (mtime changed), the filter compares the deployed bytes to a cached baseline:

    - Fast path: deployed matches baseline → echo stdin unchanged. Microseconds.
    - Slow path: deployed diverges → rehydrate the cached render, generate a diff against the baseline, apply that diff to the template source, emit the patched form to stdout.

    Result: `git status` / `git diff` show the deployed-side edit as if it had been written to the template source. The actual template file on disk is untouched — the filter only changes what git sees while reading.

    The smudge half is `cat` (identity). Templates must never re-render at smudge time — that would re-trigger secret-provider auth on every `git checkout`.

    Flags (when invoked manually for inspection):

        | Flag             | Effect                                                                    |
        | `--path <PATH>`  | Working-tree path of the file being filtered (git's `%f`). Required.      |

    :: table align=ll ::

    Manual invocation example (rare — usually for diagnosing filter behavior):

        cat my-template.tmpl | dodot template clean --path my-template.tmpl

    :: shell ::

3. Why two commands and not one

    `template install-filter` is administrative — you run it (or let the install ladder run it) once per machine to register the filter.

    `template clean` is the filter itself — git calls it on every working-tree read for `*.tmpl` files. Splitting them keeps the filter binary surface small and the install behavior separately testable.

4. Watch out for

    - *No `template show-filter` command yet.* If you want to inspect the `.git/config` block before installing, read `.git/config` directly or check `dodot template install-filter` output (which reports what was written or that it was already in place).
    - *Filter degrades gracefully.* `template clean` refuses to fail except on hard I/O. Missing baselines, decoding hiccups, even malformed cached bytes degrade to "echo stdin" with a stderr warning. Better the user sees the unmodified template through git than the entire repo becomes unreadable because of a filter bug.
    - *No reverse-side filter.* The smudge is `cat`. Reverse-merging deployed-side edits into the template *source on disk* (not just what git sees) is the job of [./transform.lex] — specifically `dodot transform check`, which is what the pre-commit hook calls.
    - *`dodot` must be on `$PATH` for whatever invokes git.* Same caveat as the plist filters — see [./git-augmentation.lex] §7.
