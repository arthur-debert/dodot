:: verified ::
Safety Lock — approving dotfiles roots

dodot resolves which directory is your dotfiles root by checking `$DOTFILES_ROOT`, then the enclosing git repository's top level, then the current directory (see [./glossary/dotfiles-root.lex]). Convenient — and one `cd` away from pointing a mutating command at a repository you never meant as dotfiles. Safety Lock is the guard on that mistake: the first time a mutating command would run against an *implicitly discovered* root you haven't approved, dodot stops, shows you the root and what it recognizes there, and asks.

1. When you'll see the prompt

    All three must hold:

    - The command mutates state the root selects — deploying, tearing down, writing into the repo.
    - The root was discovered implicitly (git top level or current directory), not set via `$DOTFILES_ROOT`.
    - You haven't approved that root before.

    The prompt shows the canonical root path, how it was selected, handler counts, and a short prioritized sample of the files dodot recognizes — enough to catch an obviously wrong directory. Only `y` or `yes` (any case) approves; Enter, any other answer, EOF, or Ctrl-C refuses, exits unsuccessfully, and changes nothing. The prompt needs a real terminal on both stdin and stderr — piped input cannot approve, and structured output (`--output json`/`yaml`) stays clean because all confirmation traffic is on stderr.

    Approval records the *path*, not the contents: it means "this directory is my dotfiles repo," survives a failed deploy, and is asked once per root. Approvals live in dodot's own data directory (`safety-lock.toml`), never in your repo.

2. What is gated, what is not

    Gated (mutating forms only):

    - `up`, `down` — deployment and teardown.
    - `init`, `fill`, `adopt`, `addignore` — repo writes.
    - `config set`, `config unset` — persist into the root's `.dodot.toml`.
    - `refresh`, `transform check` — write source files inside the root.
    - `git-install-filters`, `template install-filter`, `transform install-hook` — write the repo's `.git`.
    - The tutorial's real deployment step.

    Never gated, and never establishing trust:

    - Everything read-only: `status`, `list`, `probe`, `config list`/`get`/`gen`/`schema`, `git-show-*`, `transform status`, `secret *`, `prompts list` — so you can inspect an unfamiliar root before deciding anything.
    - Every documented preview: `up --dry-run`, `down --dry-run`, `adopt --dry-run`, `refresh --list-paths`, `transform check --dry-run`.
    - Mutations the root did not select: `install --write` (your shell rc), `git-install-alias` (ditto), `prompts reset`, factory `reset`, and the `plist`/`template clean` git-filter passthroughs.
    - `roots list` / `roots forget` — the management surface itself resolves no root, so a broken `$DOTFILES_ROOT` can't lock you out of it.

3. Automation and scripts

    Set `$DOTFILES_ROOT` to the root you mean. A valid explicit value is the deliberate selection: it never prompts and records no approval. An invalid one is a hard error naming the path — dodot will not fall back to git or the current directory, because that would mutate a root you didn't name.

    A previously approved root also proceeds without a prompt, so interactive first-runs don't break later cron jobs run from inside the repo.

4. Inspecting and revoking

    - `dodot roots list` — every approved root and whether it still exists.
    - `dodot roots forget <path>` — revoke one; the next mutating command from that root asks again.

    See [./commands/roots.lex]. Approvals are keyed to the canonical path: moving a root means the new location is unapproved (and the old approval is inert — forget it at your leisure). Factory `dodot reset` removes the approvals file along with the rest of dodot's state, even when the file is damaged beyond parsing. After reset, the next root-sensitive mutation on each implicitly discovered root asks again.

5. Two roots, one machine

    Each root is approved independently. Commands that write *source files* from dodot's shared cache — `refresh` and `transform check` — only touch sources inside the root you're running against; a baseline whose source belongs to another root is reported (`other root … not touched`) and left alone.

6. See also

    - [./commands/roots.lex] — the management commands.
    - [./glossary/dotfiles-root.lex] — how the root is resolved.
    - [./troubleshooting.lex] §10 — recovery patterns, including refusals.
