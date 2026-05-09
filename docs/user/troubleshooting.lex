:: verified ::
Troubleshooting

    Something's not behaving the way you expected. This doc is the symptom-first map: pick the row that matches what you see, follow the pointer, fix it. For deeper introspection — exactly what dodot wrote to disk, per-source shell timings, app-support routing — see [./commands/probe.lex] (summarized in §6 below).

    :: note :: Terminology — this doc uses [pack], [handler], [dotfiles root]. See [./glossary/].

1. When you reach for this doc

    - You ran a dodot command and got a result you didn't expect.
    - A file is reporting `pending` / `skipped` / `gated out` / `error` and you want to know why.
    - A deployed config isn't behaving like the one in your repo.
    - Shell startup feels slow, or a sourced file is silently failing.
    - `git diff` shows binary garbage on a plist file.

2. The three diagnostic commands

    Before reading further, try these three in order. Most surprises resolve with the first or second:

        dodot status                 # what dodot sees per pack
        dodot up --dry-run           # what dodot would do, no changes
        dodot probe deployment-map   # every symlink dodot owns, source -> target

    :: shell ::

    `status` and `up --dry-run` answer "what does dodot think?" `probe deployment-map` answers "what's actually on disk?" The gap between those two is where most surprises live.

3. "Nothing happened" / "My pack isn't visible"

    3.1. `dodot list` doesn't show the directory

        - *Is there a `.dodotignore` marker inside?* Pure file-presence check; remove with `rm <dir>/.dodotignore` to bring the pack back into discovery. See [./filters.lex] §3.
        - *Is the directory at the dotfiles root?* dodot only discovers top-level directories of `$DOTFILES_ROOT`. Nested directories (`packs/nvim/`) aren't packs unless you set the root accordingly.
        - *Is dodot looking at the right root?* dodot resolves it as `$DOTFILES_ROOT` → `git rev-parse --show-toplevel` → cwd. Run from inside your dotfiles repo, or set `DOTFILES_ROOT` explicitly. See [./glossary/dotfiles-root.lex].

    3.2. A file is in the pack but not in `dodot status`

        - It matched `[mappings] ignore` (silent drop). Files in this list never surface. See [./filters.lex] §5.
        - It matched `[pack] ignore` (scan-time drop). Default list covers `.git`, `node_modules`, swapfiles, etc. See [./filters.lex] §4.

    3.3. A pack appears as `Inactive on this OS`

        The pack has `[pack] os = ["..."]` set and your current OS isn't in the list. The whole pack is short-circuited at scan time. See [./conditional-running.lex] for the gating mechanism.

4. "Status shows it but it didn't deploy"

    4.1. Status row says `skipped`

        Matched `[mappings] skip` (defaults: README, LICENSE, CHANGELOG, …). Override per-pack with `[mappings] skip = []` to deploy intentionally. See [./filters.lex] §6.

    4.2. Status row says `gated out (<label>)`

        The file's host predicate doesn't match this machine. `install._darwin.sh` on Linux, `_linux/foo` on macOS, etc. The footnote shows expected vs actual host facts. See [./conditional-running.lex].

    4.3. Status row says `error`

        Something blocked deployment. Common causes:

        - *Routing override conflict* — a file has both a `[symlink.targets]` entry and a routing prefix (`home.X`, `_home/X`, `app.X`, …). Pick one. See [./paths.lex] §6.
        - *Protected path* — the deploy target is in `[symlink] protected_paths` (SSH private keys, `.gnupg`, AWS credentials, …). Either rename the file out of the conflict, or override `protected_paths`.
        - *A non-dodot file or symlink is at the deploy target.* Move or remove it; dodot won't overwrite files it doesn't own.

5. "It went somewhere unexpected"

    5.1. File deployed under `~/.config/<pack>/...` instead of `~/.X`

        The default rule namespaces under XDG. Use the `home.X` filename prefix, or rely on `force_home` defaults (covers `bashrc`, `zshrc`, `ssh`, `aws`, …). See [./paths.lex] §4.1.

    5.2. Deployed under `~/.config/<pack>/` instead of `~/.config/<some-other-name>/`

        Pack-name namespacing is in the way. Use `_xdg/<target>/` for a subtree, or `xdg.X` for one file. See [./paths.lex] §4.3.

    5.3. macOS: deployed under `~/.config/Code/...` instead of `~/Library/Application Support/Code/...`

        `app_support_dir` is what controls this. On macOS it defaults to `~/Library/Application Support`; if your config has `[symlink] app_uses_library = false`, that route collapses onto `~/.config`. Check `dodot config get symlink.app_uses_library`.

    5.4. "How do I confirm where it actually went?"

        ```
        dodot probe deployment-map | grep <pack>
        ```

        The source path is on the left, deployed target on the right.

6. Probe — when status isn't enough

    `dodot status` shows the per-pack story; `dodot probe` opens up the data that backs it. Four subcommands:

        | Subcommand          | Answers                                                                |
        | `deployment-map`    | Every dodot-owned symlink, source -> target.                          |
        | `show-data-dir`     | Tree view of `$XDG_DATA_HOME/dodot/` (sentinels, staged init scripts). |
        | `shell-init`        | Per-source timings + exit codes from your most recent shell startup.   |
        | `app` (macOS)       | App-support routing for a pack: folders, casks, bundles.               |
    :: table align=ll ::

    Reach for `probe` when:

    - A deployed config isn't behaving and you want to confirm the symlink chain.
    - Shell startup feels slow: `dodot probe shell-init --runs` aggregates the last 10 runs (p50 / p95 / max per source).
    - A sourced shell file is silently failing: `dodot probe shell-init --errors-only` lists every target with a non-zero exit.
    - You're routing a macOS GUI-app pack and want to confirm the resolver picked the folder you expected: `dodot probe app <pack>`.

    Full probe reference: [./commands/probe.lex].

7. Shell integration issues

    7.1. "Aliases / PATH additions from a pack don't take effect"

        - Did you add `eval "$(dodot init-sh)"` to your rc? See [./shell-integration.lex] §3.
        - Is the eval in a per-session file (`~/.bashrc`, `~/.zshrc`) and not a login-only file (`~/.profile`)?
        - Did you open a new shell? Already-running shells hold their old environment.
        - Check the script: `dodot init-sh | less`. Does it list the source you expected?

    7.2. "A sourced script is failing silently"

        Failures are not silent. The init script prints `dodot: shell source exited <code>: <path>` to stderr when a source errors. If you don't see that output, the file isn't being reached — start with `dodot init-sh` to confirm it's listed.

        For per-run history: `dodot probe shell-init --errors-only`.

    7.3. "Slow shell startup"

        ```
        dodot probe shell-init --runs        # p50 / p95 / max per source
        dodot probe shell-init --runs 50     # over the last 50 runs
        ```

        Look for sources with high p95 — that's your bottleneck. The probe needs `[shell_init].profiling.enabled = true` to collect data; if no profiles exist, open a new shell first.

8. macOS-specific issues

    8.1. "git diff shows binary garbage on a `*.plist` file"

        The dodot-plist git filter isn't installed in this clone. Run `dodot git-install-filters`. See [./plists.lex] §3.

    8.2. "I committed a plist but the app on another machine still shows old settings"

        macOS's `cfprefsd` daemon caches plist values in memory. After `git pull` + `dodot up`, run `killall cfprefsd` (auto-respawns immediately). Some apps also need a relaunch. See [./plists.lex] §6.

    8.3. "`~/Library/Containers/<...>` adopt is refused"

        Sandboxed-app data lives in Containers and isn't safe to externalize — apps may rebuild it on launch. Use `~/Library/Application Support/<App>/` (which adopts via `_app/`) or whatever the app's documented config path is. See [./adopting.lex] §3.4.

    8.4. "I'm on macOS but my GUI app config went to `~/.config/` instead of `~/Library/`"

        Either `[symlink] app_uses_library = false` is set somewhere in your config (collapsing `app_support_dir` onto XDG), or your pack name doesn't match `force_app` and there's no `_app/` prefix or `[symlink.app_aliases]` entry. See [./paths.lex] §4.4.

9. Template & secret errors

    9.1. "template render failed: undefined value"

        A template references a variable that isn't defined. The error names the file. Three fixes:

        - Define the variable in `[preprocessor.template.vars]` in your `.dodot.toml`.
        - Reference an environment variable: `{{ env.NAME }}`.
        - Mark the value optional: `{{ name | default("fallback") }}`.

        See [./templates.lex] §5.

    9.2. "secret provider 'X' is not authenticated"

        - `op` (1Password): set `OP_SERVICE_ACCOUNT_TOKEN` in your environment.
        - `bw` (Bitwarden): run `bw unlock`, export the printed `BW_SESSION`.
        - `gpg`: cache the passphrase by decrypting one file interactively first (gpg-agent will hold it).
        - `pass`: ensure `pass init <gpg-key>` has run; the password store needs to exist.

        For an at-a-glance view: `dodot secret probe`. See [./secrets.lex] §9.

    9.3. "secret resolved to a multi-line value"

        Value injection refuses multi-line secrets — they're a footgun in templated config (a single newline can break TOML / YAML / shell). Use whole-file decryption (`*.age` / `*.gpg`) for multi-line secrets. See [./secrets.lex] §4.

10. Recovery patterns

    10.1. "I added `.dodotignore` to a deployed pack and now the symlinks won't go away"

        `dodot down` only walks discovered packs; the marker hides the pack from discovery. Recovery: remove the marker, run `down`, re-add the marker.

            rm <pack>/.dodotignore
            dodot down <pack>
            touch <pack>/.dodotignore

        :: shell ::

        See [./filters.lex] §9 for the watch-out.

    10.2. "I want to start over with a clean state for a pack"

            dodot down <pack>
            dodot up <pack>

        :: shell ::

        `down` cleans every dodot-owned artifact for the pack (symlinks, install sentinels, brew sentinels, staged shell init lines). `up` rebuilds from the current pack contents.

    10.3. "I want to undo an `adopt`"

        There's no `dodot un-adopt`. To reverse:

            mv <pack>/<adopted-relative-path> <original-source-path>

        :: shell ::

        i.e. replace the symlink at the source location with the file from the pack. dodot doesn't track adoption history; git does.

11. See also

    - [./commands/probe.lex] — every probe subcommand in detail.
    - [./commands/status.lex] — the per-pack snapshot, baseline diagnostic.
    - [./paths.lex] — where files end up at deploy time.
    - [./filters.lex] — the five mechanisms for keeping files out of dispatch.
    - [./shell-integration.lex] — the eval line and what runs from it.
    - [./templates.lex], [./secrets.lex] — preprocessing-pipeline issues.
    - [./plists.lex] — macOS plist filter setup.
    - [./glossary/] — terminology.
