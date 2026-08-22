:: verified ::
dodot adopt

The "bring this existing config under dodot's control" command. For each source path you point it at, dodot moves the file into the right pack and replaces the original location with a symlink back. Nothing observable to your tools should change; the file just gained a versioned home in your dotfiles repo.

Pack name is inferred from the source's deployed location when it can be — pass `--into <pack>` to force a destination instead.

1. When you reach for it

    - Pulling legacy `$HOME` dotfiles (`~/.bashrc`, `~/.zshrc`, `~/.gitconfig`, …) into a pack for the first time.
    - Promoting an XDG config you've been editing in place (`~/.config/nvim/init.lua`) into your repo without breaking your editor session.
    - Bringing a macOS GUI app's config (`~/Library/Application Support/Code/User/settings.json`) under dodot.
    - Bulk-adopting a whole config tree: `dodot adopt ~/.config/helix/` enumerates the children and adopts each as a top-level pack entry.

2. What it does

    For each source path:

    - Computes the pack-relative target path so re-deploying via `dodot up` would land the symlink back at the original source location (the inference is the inverse of the symlink resolver's priority ladder).
    - Moves the source into the pack at that path.
    - Replaces the original location with a symlink to the moved file.

    `adopt` doesn't run handlers, doesn't update the datastore, and doesn't deploy anything. The next `dodot up` is what wires the adopted file into the deployment chain.

    Every check that can refuse the run happens before anything lands in a pack, whichever pack the files are headed for. `adopt` resolves the destination, checks each source, and looks for deployment conflicts with your other packs while the prospective content sits in a staging directory named `.dodot-adopt-<id>` inside your dotfiles root — a name pack discovery skips, so a `dodot status` running at the same time reports your packs and not a half-copied one. A refusal removes that directory and leaves your repo as it found it, an existing pack byte-identical and an inferred one still absent.

    What differs is how the staged content reaches the pack:

    - *A pack whose name was inferred and doesn't exist yet.* The pack is created by publishing the staged tree with a single rename, so it appears complete or does not appear at all. If something else created that pack path while the run was preparing, publication refuses rather than writing over it.
    - *A pack that already exists.* Each entry is renamed into place one at a time, and under `--force` an existing destination is moved into the staging directory first. Each file appears atomically; the run as a whole is *recoverable* rather than atomic. If any entry fails to publish, `adopt` puts back every entry it had already moved — including every destination `--force` displaced — removes the directories it created inside the pack and left empty, and tells you what it restored. If a step of that recovery fails in turn, `adopt` names the entries it could not put back and where their content is now, and keeps the staging directory rather than cleaning it up — nothing is deleted, and you move what you need back by hand.

    Once publication succeeds, whatever `--force` displaced stays in the staging directory while your original files are replaced by symlinks, and is discarded once every replacement has been attempted. That discard is where `--force` takes effect. Note the boundary: a *replacement* that fails is reported and takes its own pack entry back out, but it does not put its displaced destination back — that recovery isn't built yet, so on a `--force` run a failed replacement loses the destination's old content. Publication failures are recovered; replacement failures are not.

    Recovery lasts as long as the process does. A `dodot adopt` killed partway through publishing into an existing pack leaves that pack holding some of the run's entries and leaves its `.dodot-adopt-<id>` directory on disk with whatever was displaced still inside. Nothing later touches either: pack discovery does not read the staging directory, and no subsequent `adopt` publishes from it or deletes it. Inspect it and remove it yourself.

3. Pack inference

    Pack name is inferred from the source's deployed location:

        | Source root                                  | Pack name        | In-pack path                    |
        | `$XDG_CONFIG_HOME/<X>/<rest>`                 | `<X>`            | `<rest>`                        |
        | `$HOME/.<X>` (file, not on `force_home`)      | require `--into` | `home.<X>`                      |
        | `$HOME/.<X>/...` (dir, not on `force_home`)   | require `--into` | `_home/<X>/...`                 |
        | `$HOME/.<X>` (entry on `[symlink].force_home`)| require `--into` | `<X>` (no prefix)               |
        | `~/Library/Application Support/<X>/<rest>`    | `<X>`            | `_app/<X>/<rest>`               |
        | `~/Library/<sub>/<file>` (other macOS Library)| require `--into` | `_lib/<sub>/<file>`             |
        | `~/Library/Containers/...`                    | refused          | (sandboxed-app data — refused)  |

    :: table align=lll ::

    See [./../../reference/symlink-paths.lex] for the full inference table including edge cases.

    HOME-rooted sources don't auto-infer because `~/.bashrc` could plausibly belong in any of `shell`, `bash`, `dotfiles`, or whatever you call the pack — there's no path structure to read the name from. `--into` is the explicit answer to that question.

    Multi-source consensus: when you pass several sources at once, all per-source inferences must agree on a single pack name (or all decline; in that case `adopt` errors pointing at `--into`). `dodot adopt ~/.config/nvim/init.lua ~/.config/nvim/lua/` works without `--into` because both infer `nvim`; mixing roots without `--into` does not.

4. The `--into` override

    `--into <pack>` overrides per-source inference and routes every adopted source into one named pack. The in-pack path adapts:

    - XDG source, `--into` differs from inferred name: in-pack path becomes `_xdg/<original-pack-segment>/<rest>` so the deploy round-trips. Useful for grouping unrelated XDG configs into a single pack: `dodot adopt --into tools ~/.config/lazygit/` lands at `tools/_xdg/lazygit/`.
    - HOME source: `home.X` and `_home/X/` are pack-name independent, so the in-pack path is the same regardless of which pack you target.
    - AppSupport source, `--into` differs: uses `_app/<X>/<rest>` analogously to XDG.

    `--into` does *not* create the pack if it doesn't exist — you must run `dodot init <pack>` first or the command errors. (Inferred packs *are* auto-created.)

5. What adopt will and won't adopt

    A pack scan doesn't read everything inside a pack. It skips dodot's own `.dodot.toml` and `.dodotignore`, every top-level name matching `[pack] ignore`, and every top-level name starting with `.` except `.config` (see [./../filters.lex] §4). Adopting a file into one of those positions would produce a pack entry that no later `dodot up` or `dodot status` ever reads — so `adopt` checks the same three rules, at the same positions, before it writes anything.

    What it does about a match depends on whether you named the path:

    - *You typed the path.* The run is refused. Nothing is written, and no pack is created. For an `[pack] ignore` match the error quotes the pattern and names the file that supplied the list, so you know which one to edit. For a reserved name or a hidden name there's no setting to point at, and the error says so rather than implying a fix.
    - *`adopt` found the path expanding a directory you named.* The child stays a real file at its original path, its adoptable siblings are adopted normally, and the run reports it once and exits `0`. A discovered `.dodot.toml` or `.dodotignore` is the exception — copying either into a pack would replace the pack's configuration or hide the pack, so that one refuses the run.

    The report is the only time you hear about it. `[pack] ignore` matches are invisible in `dodot status` by design, so no later command tells you `~/.config/zed/.DS_Store` is still a real file among symlinks:

        $ dodot adopt ~/.config/zed/
        left in place: /Users/you/.config/zed/.DS_Store — matches `.DS_Store` in [pack] ignore (dodot's default list)
        [... the pack's status, with the adopted entries ...]

    :: shell ::

    A directory whose children are *all* skipped is an error, not a report — reporting each child and exiting `0` would claim an adoption that never happened. The message lists every child and the rule that matched it. An empty directory is the same refusal.

    One more name is checked before any of these: the pack directory itself. `dodot up` and `dodot status` start by scanning your dotfiles root, and that scan applies its own rules to each directory name — the root `[pack] ignore` list, the same hidden-name rule, and the pack-name grammar. Since an inferred pack name comes from the source's own path, `adopt` can land on a name that scan skips: `dodot adopt ~/.config/node_modules/settings.json` would publish a `node_modules` pack, which the default ignore list makes every later run pass over. That refuses too, and the fix is to say where the file goes — `--into <pack>` names a pack dodot already reads.

    A directory named `_<label>` for a gate label nothing defines is refused for a harsher reason. A pack scan doesn't *skip* one, it stops on it, and that error fails the scan of the whole pack. So adopting `~/.config/nvim/_bogus/init.lua` would break a pack that read fine a moment earlier. Named directly, it refuses; found while expanding a directory, it stays where it is and is reported like any other left-in-place entry, which is what keeps the pack you just published readable. Defined labels — the built-ins and anything under `[gates]` — are unaffected, and so are the routing prefixes, which are not gate labels.

    Three limits are worth knowing:

    - *Only the top-level position is checked.* `dodot adopt ~/.config/nvim/lua/plugins/init.lua` lands at `lua/plugins/init.lua`, and only `lua` is classified. If `lua` matches an ignore pattern the adoption is refused; if `plugins` or `init.lua` matches, it proceeds, because the scan reads `lua` and hands the whole directory to the symlink handler. Routing prefixes (`_home/`, `_xdg/`, `_app/`, `_lib/`) sit at that position and are tested there like any other name — none of them is hidden or reserved, so none refuses on its own. A `--only-os <label>` directory is the one that goes deeper: it expands transparently on a matching host, so the name inside it is checked too.
    - *Adopt doesn't look inside a directory it copies.* `dodot adopt ~/.config/helix/themes/` copies the directory whole, ignored files included, and later scans omit them silently — the same behaviour any adopted directory has always had.
    - *Dispatch-layer filters don't participate.* A file matching `[mappings] ignore` or `[mappings] skip`, or carrying a gate label dodot defines, is discovered by the scan and then routed. It's a live pack entry whose routing you change by editing config, so `adopt` treats it as ordinary.

    `--force` changes none of this. It answers one question — may `adopt` replace an existing destination inside the pack — and answers it after the rules above have already decided which entries exist.

6. Flags

    Flags:
        | Flag             | Effect                                                                                       |
        | `--into <PACK>`  | Force a destination pack. Pack must exist. Overrides per-source inference.                   |
        | `--force`        | Overwrite an existing destination file in the pack.                                          |
        | `--dry-run`      | Show the moves and symlinks that would happen without making changes.                        |
        | `--no-follow`    | If the source is itself a symlink, move the link rather than its target.                     |

    :: table align=ll ::

7. Examples

        # XDG-rooted: pack name inferred from path
        dodot adopt ~/.config/nvim/init.lua             # pack `nvim`, in-pack `init.lua`
        dodot adopt ~/.config/helix/                    # expands children, pack `helix`

        # HOME-rooted: --into is required
        dodot adopt --into shell ~/.bashrc
        dodot adopt --into git ~/.gitconfig ~/.gitignore_global

        # Override an XDG inference into a different pack
        dodot adopt --into tools ~/.config/lazygit/     # in-pack `_xdg/lazygit/`

        # macOS Application Support: pack inferred, _app/ encoded
        dodot adopt ~/Library/Application\ Support/Code/User/settings.json

        # macOS ~/Library/<sub>/: --into required, _lib/ encoded
        dodot adopt --into mac-defaults ~/Library/Preferences/com.app.plist
        dodot adopt --into agents ~/Library/LaunchAgents/com.example.foo.plist

        # Preview before pulling the trigger
        dodot adopt --dry-run --into git ~/.gitconfig

    :: shell ::

8. Watch out for

    - *`~/Library/Containers/` is refused.* Sandboxed-app container data isn't safe to externalize — apps treat the path as private and may rebuild on launch. The error points you at the right alternative (usually `~/Library/Application Support/<App>/`).
    - *`--no-follow` is for adopting symlinks themselves.* By default, if you adopt `~/.bashrc` and it's *already* a symlink to somewhere else, dodot follows the link and moves the *target*. Pass `--no-follow` to move the symlink itself instead. Comes up when consolidating across multiple dotfiles managers.
    - *Plist tip on first adopt.* When you adopt a `*.plist` file and the dodot-plist git filter isn't yet registered, `adopt` prints a one-line tip pointing at `dodot git-install-filters`. The first `dodot up` after will offer the same install via the install ladder. See [./git-augmentation.lex].
    - *Pack must exist when `--into` is used.* Inference auto-creates new packs; explicit `--into <pack>` does not. If you're starting fresh, `dodot init <pack>` first.
    - *One invocation can't name a directory and something inside it.* `dodot adopt ~/.config/nvim/lua ~/.config/nvim/lua/init.lua` is refused before anything is written, and so is the same pair reached by expansion (`dodot adopt ~/.config/nvim ~/.config/nvim/lua/init.lua`). No order comes out right: replace the directory first and the file path now resolves back into the pack through the new symlink, so replacing it overwrites the pack's own entry; replace the file first and publishing the directory buries it. Adopt the outer path alone — adopting a directory already carries its contents. Two sources that don't contain each other can still land one inside the other in the pack — `dodot adopt --into nvim ~/.config/other/lua ~/.config/nvim/_xdg/other` puts one at `_xdg/other/lua` and the other at `_xdg/other` — and that pair is refused the same way, naming both pack paths.
    - *An `externals.toml` template dodot hasn't rendered stops the run.* Before writing anything, `adopt` checks that no other pack already claims the paths you're about to deploy to. A pack's `externals.toml` declares its targets inside the file, so `adopt` has to read it — and if that file only exists as `externals.toml.tmpl` and you have never run `dodot up` on that pack, there is nothing to read. The same applies once you edit that template, or a `vars` value it interpolates: what dodot has on hand is then the last render, whose targets may not be the ones the file now names. Rendering it here would resolve its secrets and write its output for a run you haven't agreed to yet, so `adopt` refuses instead and names the file. Run `dodot up` for that pack once, then re-run `adopt`. `--force` doesn't skip this: it overrides what dodot found in the way, not what dodot hasn't looked at.
    - *An ignored, hidden, or reserved destination refuses or reports.* `adopt` checks the rules a pack scan applies before it writes, so it never produces a pack entry — or a pack — dodot won't read. That includes the inferred pack name: `dodot adopt ~/.config/node_modules/settings.json` refuses rather than publishing a pack the dotfiles-root scan skips. §5 has the full behaviour.
    - *Adopt is reversible by hand, not by command.* There's no `dodot un-adopt`. To undo: replace the symlink at the source location with the moved file (`mv <pack>/<rel> <original>`). dodot doesn't track adoption history.
