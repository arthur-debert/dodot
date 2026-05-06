:: verified ::
dodot secret

The "is my secrets setup healthy, and what does this repo expect from it?" command. Two read-only subcommands let you inspect the secrets layer without running `dodot up`: probe each configured provider for health, and inventory every `secret(...)` call across your templates.

Both commands are advisory and safe — they never call `resolve()`, never decrypt, never prompt for passwords or keystore unlocks.

1. When you reach for it

    - Before the first `dodot up` on a new machine: `dodot secret probe` to confirm every provider you depend on is installed and authenticated. Faster than letting `up` discover the same problem.
    - You're auditing a dotfiles repo (yours or a teammate's) and want to know which providers it actually uses: `dodot secret list`.
    - You're debugging a "template render failed: secret(...) couldn't resolve" error and want to see the whole call inventory and the per-provider state side by side.

2. secret probe

    Runs `probe()` on every enabled provider configured under `[secret]` and reports one row per provider with the outcome. Always exits 0 — even a fully-broken provider lineup isn't an error here, it's information.

    Possible row states:

        | State              | What it means                                                                       |
        | `ok`               | Provider is installed, authenticated, and ready to resolve secrets.                 |
        | `not_installed`    | Provider's underlying tool isn't on `$PATH` (e.g. `op`, `gpg`, `age`).              |
        | `not_authenticated`| Provider is installed but the user hasn't unlocked it (e.g. 1Password not signed in).|
        | `misconfigured`    | Provider is installed but configuration is wrong or incomplete.                      |
        | `probe_failed`     | Provider's `probe()` call itself threw an error.                                    |

    :: table align=ll ::

    The renderer picks a different message when no provider is enabled (`[secret] enabled = false` or no providers configured) — there's nothing to probe in that case.

    Examples:

        dodot secret probe                             # provider health check
        dodot secret probe --output json | jq '.rows[] | select(.state != "ok")'

    :: shell ::

3. secret list

    Scans every pack's templates for `secret(...)` calls and reports one row per call: which template it's in, the line number, the full reference passed in. Useful for inventorying which providers a repo expects before you run `dodot up`.

    The scanner is byte-wise on the template *source* — meaning:

    - Plaintext templates are visible: `secret("op://Personal/AWS/access_key")` shows up.
    - Encrypted file content is *not* scanned. A `secret(...)` reference inside an `.age`-encrypted source isn't visible to `list`, by design — `list` doesn't decrypt.

    Examples:

        dodot secret list                              # every secret(...) call in the repo
        dodot secret list --output json                # machine-readable

    :: shell ::

4. Watch out for

    - *Never invokes `resolve()`.* Both commands are read-only at the provider boundary — `probe` calls `probe()` (a health check), `list` doesn't call providers at all. So neither will trigger a 1Password unlock prompt, a keystore decrypt, or an `age` passphrase request.
    - *`[secret]` is root-only.* Secret-provider configuration lives in the dotfiles-root `.dodot.toml`, not per-pack. Both subcommands read from root config; pack-level `[secret]` blocks are a config error elsewhere.
    - *`secret list` doesn't see inside encrypted files.* If your encrypted templates contain `secret(...)` calls (rare, but legal), they won't show up here. The encrypted file *itself* shows up in the secrets pipeline; the `secret(...)` calls inside are only visible after decrypt, which `list` never does.
    - *`secret probe` exit code is 0 even on failures.* The non-zero state is in the rows, not the process exit. If you're scripting against this, parse `--output json` and check the `failing_count` field.
