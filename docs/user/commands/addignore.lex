:: verified ::
dodot addignore

The "stop discovering this directory as a pack" command. Drops a `.dodotignore` marker file into the named directory; from that point, dodot's discovery skips it — it won't appear in `list`, `status`, or `up`.

Idempotent: safe to run again on an already-ignored directory. To reverse, remove the marker by hand: `rm <dir>/.dodotignore`.

1. When you reach for it

    - You have a directory under your dotfiles root that isn't meant to be deployed: notes, scratch space, half-finished migrations, generated artifacts, README-only directories.
    - You're temporarily parking a pack you don't want active right now and want it out of `dodot status` until you decide what to do with it.
    - You inherited a dotfiles repo with packs you don't want to use yet and want to silence them without deleting.

2. What it does

    Creates a single zero-byte file at `<pack>/.dodotignore`. dodot's discovery looks for the marker by file presence only — the contents are never read. From the next dodot invocation onward, the directory is excluded from `list`, `status`, `up`, and `down`.

3. Examples

        dodot addignore notes
        dodot addignore work-in-progress
        ls notes/.dodotignore           # the marker dodot left behind

        # Reverse it
        rm notes/.dodotignore

    :: shell ::

4. Watch out for

    - *Add the marker AFTER `dodot down`, not before.* If the directory was previously deployed, adding `.dodotignore` makes it invisible to discovery — but `dodot up` and `dodot down` only reconcile *discovered* packs, so the previously-deployed symlinks are *not* cleaned up automatically. Run `dodot down <pack>` *first*, then `dodot addignore <pack>`. See [./../handlers/controlling-activation.lex] §4.
    - *Different from `[pack] ignore`.* `addignore` skips the *whole directory* as a pack. To skip individual files *inside* a pack that's otherwise active, use `[pack] ignore` patterns in `.dodot.toml` instead.
    - *Different from `[mappings] ignore` / `[mappings] skip`.* Those drop matched source files from handler dispatch, but the file is still discovered. `.dodotignore` stops discovery one layer earlier.
    - *No `dodot addignore --remove`.* The reverse is `rm <pack>/.dodotignore` by hand. (Idempotent on add, manual on undo — fits the "git is your history" posture.)
