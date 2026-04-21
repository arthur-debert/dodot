dodot Philosophy

    dodot is opinionated. This document explains the opinions, the reasoning behind them, and the trade-offs they force. If the opinions match yours, dodot will feel obvious. If they don't, you should probably use a different tool. Either outcome is fine — the point of this document is to help you tell which one applies.

    :: note :: See [./terms-and-concepts.lex] for terminology used throughout.

1. The Problem Isn't Solved

    Configuring your tools is exacting work that you're expected to redo every time you reinstall, switch machines, or onboard a new role. Since the problem is as old as dotfiles themselves — happy 50th birthday, `.profile` — you'd expect it to have been solved once and moved on from. It hasn't. The landscape is a spectrum with two clusters and a gap.

    - At the low end: manual management — scp, tar, self-email, git plus a shell script. Perfectly workable. Error-prone, not repeatable, rarely centralized.
    - At the high end: complex tools — chezmoi, dotbot, nix-home-manager, and fleet-provisioning frameworks like Ansible. Powerful, but each imposes a workflow, a database, or a domain language.

    What's missing is the middle. You want real features — not just symlinks, but PATH management, script execution, machine-specific configs, secret handling — without signing up for a workflow migration. That middle is where dodot lives.

2. The Things dodot Refuses To Do

    These aren't preferences. They're the hard constraints that shape every design decision. Each one came out of pain with tools that crossed the line.

    2.1. Never wrap source control

        git is the source of truth. Full stop. dodot does not diff, status, commit, or otherwise interpose itself between you and git. You use git the same way you'd use it anywhere else — vanilla commands, vanilla workflow. The shelf life of a dotfile manager is a few years; git's shelf life is decades. Couple your data to git, not to us.

    2.2. No workflow change

        You edit configs the way you already edit them — in your editor, through an app's settings pane, however. There's no `dodot edit`, no wrapper shell, no forced commit step. If your tool hot-reloads on disk changes, it still does. If it reloads on next launch, it still does.

    2.3. No apply step

        Running `dodot up` once is setup. After that, edits are live — the deployed file IS the source file through a symlink chain. Forget to regenerate? There's nothing to regenerate.

    2.4. No lock-in

        Uninstalling dodot should leave a working system. The artifacts dodot creates — symlinks, a small state directory — are all legible, scriptable, and movable by hand. No bespoke format, no database, nothing that requires dodot to interpret.

3. The Things dodot Wants To Do

    The no-gos above are necessary but not sufficient. A tool that only symlinks isn't much of a tool. dodot also wants to:

    - cover the real feature set: PATH additions, shell hooks, package installs, one-shot provisioning
    - be honest about what it will do before it does it (`dodot status`, `--dry-run`)
    - keep the configuration surface small enough to hold in your head
    - make both adoption and abandonment cheap — minutes to try, minutes to leave
    - eventually cover the harder cases: template expansion, secret injection, binary-format representations like plists

    Holding these together with the no-gos is the interesting problem.

4. The Data Tension

    Here's the design problem in one paragraph. You want dodot to be able to answer: _is this file managed by me? was this install script already run? what's pending versus deployed?_ That kind of answer usually implies state — a database, a manifest, an `apply`-step output. But any bespoke state representation creates lock-in, drifts from reality, and does a poor job of reimplementing a package manager.

    We want queryable state, _and_ we want the filesystem to be the state, _and_ we don't want a database. Pick two — unless you can find a trick.

    The trick dodot uses is the _double-link_: a layer of indirection through a small, legible directory that IS the state. See [./data-layer.lex] for the mechanics. The short version:

    - queryable with `ls` and `readlink`, not a custom tool
    - scriptable: you can move, copy, or inspect it like any directory tree
    - dies cleanly when dodot dies — no orphan formats
    - lets dodot tell the difference between "file linked by me" and "file you linked yourself" without a sidecar database

    The double-link is the single most unusual thing about dodot. Everything else is consequence.

5. Live Receipts

    The double-link has a second property that's easy to miss. In most tools, state is a receipt — a record of what happened, written alongside the thing that happened. "Ran install.sh at 14:32"; "linked .vimrc to ~/dotfiles/vim/vimrc." The receipt lives in a separate place (a database, a lockfile) from the thing itself. Keeping them in sync is a problem the tool has to solve, and when it gets it wrong, you get drift — the receipt says one thing, the filesystem says another.

    dodot's datastore inverts this. The state isn't a record of what happened; it IS what happens. When the shell integration runs at login, it walks the datastore directory: for every file under `<pack>/shell/`, it emits a `source` line; for every directory under `<pack>/path/`, it prepends that directory to `$PATH`. The state directory is the _input_ to the behavior, not a description of it.

    That makes every entry a _live receipt_: it simultaneously records that dodot did something and produces the effect of that thing. Delete a symlink from the datastore and the corresponding config stops being live on the next shell open — no cleanup step, no re-sync. Add one by hand and it starts being live. You can't drift because there's nothing to drift from.

    This is the property that makes `ls -la ~/.local/share/dodot/packs/` a real debugging tool. You aren't reading a log of past dodot runs; you're reading the current, effective configuration. The receipt and the thing are the same object.

6. Conventions Over Configuration

    Once you go beyond "just symlink it," a tool has to decide what to do with `install.sh`, `aliases.sh`, `Brewfile`, `bin/`. The expressive path is a DSL — a manifest file that lists, per file, what happens. dodot's path is convention: file names map to handlers. You already name your files sensibly; let that be the config.

    This buys two things.

    - _Zero-config onboarding._ A dotfiles repo that already uses conventional names — and almost all of them do — needs no setup to work with dodot.
    - _Trivial extensibility._ Add a new handler, add a default name mapping. Users who want different names drop a `.dodot.toml`; nobody else notices.

    The trade-off is collisions between communities' conventions. We picked names that are widespread and not too contested; where we had to pick, we picked. You can always override.

7. Trade-offs We Accept

    Design is subtraction. The things dodot is deliberately not:

    - _No profiles._ One configuration per machine. For "work vs home," use separate packs and enable different subsets.
    - _No rollback outside git._ dodot does not keep a history of past deployments. Git keeps history of your configs; that is the history you want.
    - _No central orchestration._ dodot is a single-user, single-machine tool. Fleets belong to Ansible and friends.
    - _Unopinionated about structure._ dodot won't tell you how to organize packs. That's your judgment call; the tool works with whatever directories it finds.

    Several of these could be added as features. We think they would cost more in complexity than they'd return in utility for the target user.

8. When dodot Fits

    You are likely to like dodot if:

    - your dotfiles live in git and you want that to stay true
    - you've been burned by tools that wrapped your workflow
    - you want more than symlinks (PATH, shell, install scripts) without a DSL
    - you expect to use your dotfile setup for years, across several machines, and don't want a migration when the current tool goes stale

    dodot is probably the wrong fit if:

    - you need multiple profiles on one machine
    - you want rollback of deployment history as a first-class feature
    - you're doing fleet provisioning (Ansible, Salt, Nix are more appropriate)
    - you need hard guarantees about secrets that go beyond "don't commit plaintext"

9. The Magical Extras

    Some features live at the edge of "can we keep the no-gos?" Template expansion, secret injection, plist handling — these all involve a source file that differs from the deployed file, which is exactly the case where the simple symlink story breaks down.

    dodot's answer is a preprocessing pipeline plus some careful work on the git side (clean/smudge filters, a pre-commit reverse-merge step) to preserve the "git is truth, no workflow change" contract even when the deployed file and the source file diverge. The design is detailed in [./pre-processors.lex] and in the [./../proposals] directory.

    The point to take from this document is that these features exist because giving them up would mean users go elsewhere, and they are designed with the no-gos of section 2 treated as hard constraints, not negotiable trade-offs.

10. The Bottom Line

    dodot does the minimum to be useful, and then carefully adds what's needed to stay useful for years. It is opinionated about the inputs — git, filesystem, shell — and quiet about everything else. Your dotfiles are yours; dodot helps put them in the right places and gets out of the way.
