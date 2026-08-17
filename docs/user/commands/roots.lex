:: verified ::
dodot roots

The "which directories have I told dodot are mine?" command pair. Safety Lock records an approval the first time you confirm a mutating command on an implicitly discovered dotfiles root ([./../safety-lock.lex]); `roots` is where you inspect and revoke those approvals.

There is no `roots trust` — approval only happens at the point of use, where the prompt can show you what dodot found there.

1. When you reach for it

    - You want to see which roots this machine has approved: `dodot roots list`.
    - You no longer trust a directory (or approved the wrong one): `dodot roots forget <path>`.
    - A root moved: the old approval is inert (approvals are keyed to the canonical path), so `forget` the stale entry when the list bothers you.

2. Subcommands

    Subcommands:
        | Subcommand        | Effect                                                                              |
        | `list`            | Show every approved root, and whether it still exists on disk.                      |
        | `forget <path>`   | Revoke one approval; the next mutating command from that root asks again.           |

    :: table align=ll ::

    `forget` takes the path as `roots list` prints it; an existing path is canonicalized first, so `dodot roots forget .` from inside the root works. Forgetting a path that matches no approval reports that and exits 0.

3. Examples

        dodot roots list
        dodot roots forget ~/scratch/some-repo
        dodot roots forget .           # revoke the root you're standing in

    :: shell ::

4. Watch out for

    - *Neither subcommand resolves a dotfiles root.* They manage the approval collection itself, so they work even when `$DOTFILES_ROOT` is set to something broken — this surface must never be lockable by the very configuration it exists to repair.
    - *An unreadable approvals file is an error here, not an empty list.* "Nothing approved" and "dodot could not read the trust state" are different answers, and `roots list` refuses to blur them. Factory `dodot reset` removes a damaged file.
    - *Revocation is per root, per machine.* Approvals live in dodot's data directory, not in the repo, and say nothing about other machines.
