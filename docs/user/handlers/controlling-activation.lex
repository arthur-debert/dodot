:: verified ::
Controlling activation — keeping source files out of dispatch

By default, dodot processes every directory it finds under the dotfiles root and dispatches every source file inside to a handler. This snippet covers the four ways to opt out: drop a source file silently (`ignore`), drop it but list it (`skip`), drop it conditionally on host facts (`gate`), and skip whole packs or files at discovery time (`[pack] ignore` and `.dodotignore`).

1. The three filter handlers

    Three handlers in the filter phase exist solely to keep source files away from the deploying handlers. They differ in *visibility* and in *why* the source was kept out.

    1.1. ignore

        Claims source files matching the configured patterns and drops them silently — same contract as `.gitignore`. No entry in `dodot status`, no executable intent.

        Configured via `[mappings] ignore` (default empty). Useful for build artifacts, scratch files, anything you don't want dodot to know about.

            [mappings]
            ignore = ["*.bak", "scratch.txt"]

        :: toml ::

    1.2. skip

        Claims source files matching the configured patterns, surfaces them in `dodot status` as `skipped`, but produces no executable intent — `dodot up` will not deploy them.

        Configured via `[mappings] skip`. The defaults cover the common documentation and legal files that packs ship alongside real config (`README`, `LICENSE`, `CHANGELOG`, `CONTRIBUTING`, `AUTHORS`, `NOTICE`, `COPYING`, plus their `.*` variants), matched case-insensitively against the basename. Override per-pack with `skip = []` to deploy a `README` intentionally, or replace the list to use your own conventions.

            # deploy our README, but skip TODO.md
            [mappings]
            skip = ["TODO.md"]

        :: toml ::

    1.3. gate

        Claims source files whose host predicate evaluates false on this host — e.g. `install._darwin.sh` on a linux box, or anything inside `_darwin/` when running on linux. Surfaces in `dodot status` as `gated out (<label>)` with a footnote showing the expected vs actual host facts. Produces no executable intent; the source file stays on disk and will deploy on a matching host.

        Unlike `ignore` and `skip`, gate matches are *dynamic* — they depend on host facts (OS, arch, hostname, …) and on the filename grammar (`._<label>`, `_<label>/`) plus the `[mappings.gates]` config. The full surface lives in [./../conditional-running.lex]; the short version:

            install._darwin.sh                # filename suffix: gated by `darwin`
            _darwin/foo.sh                    # directory segment: gates the whole subtree
            [pack] os = ["darwin"]            # whole-pack gate (in pack `.dodot.toml`)
            [mappings.gates]                  # glob escape hatch (in pack `.dodot.toml`)
              "install-mac.sh" = "darwin"

        :: text ::

2. Pack-level discovery exclusion

    `[mappings] ignore`, `[mappings] skip`, and gate operate one layer down — the source file is *discovered*, then a filter handler claims it before any deploying handler sees it. The two pack-level mechanisms below stop discovery instead, so matched paths never become candidates for any handler at all.

    2.1. `[pack] ignore`

        The broadest in-pack hammer. Its glob patterns are excluded from pack discovery and file scanning entirely.

        Defaults cover version-control noise, editor swapfiles, and common build artifacts:

            [pack]
            ignore = [
                ".git", ".svn", ".hg",
                "node_modules", ".DS_Store",
                "*.swp", "*~", "#*#",
                ".env*", ".terraform",
            ]

        :: toml ::

        These are project-config noise; you almost never want dodot to know they exist. Override the list if you need to.

    2.2. `.dodotignore`

        A marker file inside a pack directory that tells dodot to skip the whole pack — discovery doesn't recurse into the directory at all. Pure file-presence check; the file's contents are never read.

        Useful for directories that live in your dotfiles repo but aren't meant to be deployed: scratch space, notes, README-only packs, work-in-progress.

3. Choosing between them

    What you want versus where to set it:

        | You want…                                              | Use                                    |
        | The whole pack invisible to dodot                       | `.dodotignore` marker file             |
        | A source file invisible during pack scanning            | `[pack] ignore`                        |
        | A source file invisible to handler dispatch (silent)    | `[mappings] ignore`                    |
        | A source file visible in `dodot status`, undeployed     | `[mappings] skip`                      |
        | A source file deployed only on certain hosts            | gate (filename, directory, or `[pack] os`)  |

    :: table align=ll ::

    When more than one filter could match, `ignore` wins over `skip` (silent-drop is the stronger signal); both win over precise mappings (`shell`, `install`, …) and the catch-all symlink. A gated-out source file behaves like `skip` in `dodot status` — visible, not deployed — but specifically because the host doesn't match, not because the user marked it for documentation skipping.

4. Live edits

    Filter and gate decisions are recomputed on every `dodot up`, `dodot status`, and `dodot down` — there's no stored "filter state." Edit `[mappings] ignore`, `[mappings] skip`, `[mappings.gates]`, or `[pack] ignore` and the change takes effect on the next dodot invocation.

    For `[mappings]` and `[mappings.gates]` filters, `dodot up` reconciles per-pack state on every run, so adding (say) `*.bak` to `[mappings] ignore` cleans up any previously-deployed `*.bak` symlinks for that pack on the next `up`.

    `.dodotignore` is the exception. Dropping the marker into a previously-deployed pack stops it from being discovered on the next `dodot up` — but pack discovery is also what `up` and `down` walk to find packs to reconcile, so the pack's previously-deployed symlinks are *not* cleaned up automatically. Run `dodot down <pack>` *before* dropping the marker (or remove the marker, run `down`, re-add) to clean up the deployed state.

    Removing `.dodotignore` brings the pack back into discovery; the next `dodot up` reconciles and re-deploys it normally.
