Preprocessors

    Some dotfiles need to vary between machines, inject secrets at deploy time, or exist in one format under source control and a different one on disk. For these cases a plain symlink is not enough: the file in your repo is not the file you want deployed. Preprocessors are how dodot handles that without giving up the properties that make dodot dodot.

    This document covers what a preprocessor is, the three shapes they come in, and how they fit into the rest of the pipeline. For hands-on template usage, see [./../user/templates.lex]. For the design of unshipped preprocessors, see the [./../proposals] directory.

    :: note :: See [./terms-and-concepts.lex] for terminology used throughout.

1. The Problem

    The default dodot story is: your repo file IS your deployed file, linked through the datastore. That works beautifully when the two really are the same bytes. It breaks down whenever they're not.

    - Templates. You want the git-user email to differ between laptop and workstation. You need _one_ source file, _two_ different deployed files.
    - Secrets. Your shell config references an API token you don't want in git.
    - Binary representations. macOS plists live as binary on disk but are useless under source control in that form.
    - Encrypted artifacts. An SSH key belongs on disk in plaintext but in git encrypted.

    All of these share a shape: the version-controlled source and the deployed artifact differ. The question is how to handle that without giving up git-is-truth, no-apply-step, or no-workflow-change.

2. What a Preprocessor Is

    A preprocessor is a phase in the dodot pipeline that runs _before_ handler dispatch. It takes a source file and produces an output file. The output is written to the datastore, where handlers pick it up as if it had always been there.

    Preprocessors are identified by filename convention, the same way handlers are. `config.toml.tmpl` is a template (preprocessor: template; output: `config.toml`). `com.app.plist.xml` is a plist source (preprocessor: plist; output: `com.app.plist`). `id_ed25519.age` is an encrypted blob (preprocessor: age; output: `id_ed25519`).

    What comes out the other side of the preprocessor looks to the rest of the pipeline like a regular file. The handler that claims it doesn't know or care that preprocessing happened. This matters for composition: a rendered `aliases.sh.tmpl` still flows through the shell handler and gets sourced at login; a rendered `install.sh.tmpl` still runs once and stores a sentinel.

3. Three Shapes of Transformation

    Preprocessors differ not in what they do — they all take a source and produce an output — but in whether the transformation can be reversed. This determines what happens when the _deployed_ file is edited (by an app, by you, by a GUI settings pane) and no longer matches what the preprocessor would produce from the source. dodot calls this _divergence_, and divergence has to be handled differently depending on the transform's shape.

    3.1. Generative (one-way)

        A template produces its output, but the output doesn't contain enough information to reconstruct the template. `{{ hostname }}` becomes `laptop` on render; from `laptop` alone you cannot tell whether the template said `{{ hostname }}`, a hard-coded literal, or something else entirely.

        Source of truth: the source. The deployed file is a derived artifact.

        Divergence: handled best-effort. dodot can tell you the deployed file changed, and for the common case — edits to lines that don't touch a template expression — it can confidently write those changes back to the source. For lines that do touch a template expression, it cannot; it inserts a conflict marker and asks you to decide.

    3.2. Representational (two-way)

        A plist can be converted from binary to XML and back with no data loss; the two are equivalent representations of the same data. The same is true of many format pairs.

        Source of truth: whichever form the tooling modifies. For plists, the app writes the binary continuously, so the binary is authoritative at any given moment; the XML is the git-friendly mirror.

        Divergence: handled automatically. The exact mechanism depends on the preprocessor — for plists, it's git's own clean/smudge filter machinery (see [./plists.lex]); for any future Representational preprocessor running through the pipeline, it would be a `dodot transform check` pass that reverse-converts and writes back. Either way, no heuristics are needed and no user-facing conflicts arise — the math is on our side.

    3.3. Opaque (write-only)

        An encrypted file can be decrypted on deploy, but re-encrypting it silently from a changed plaintext is neither safe nor dodot's job. The deployed form is plaintext; the source form is ciphertext; the reverse path is deliberately left out.

        Source of truth: the source (ciphertext). The deployed file is ephemeral.

        Divergence: reported, not acted on. If the deployed plaintext has been modified, dodot tells you; you decide how to update the source, decrypt it yourself, edit it, re-encrypt, and commit. dodot does not own your encryption keys.

    These three shapes cover the full spectrum from "fully automatic" (plists) to "hands off" (encryption). Every preprocessor declares which shape it is, and the pipeline adjusts its divergence handling accordingly.

4. Why Preprocessing Is a Phase, Not a Handler

    Preprocessors could have been built as handlers — match `.tmpl`, do the rendering, then... deploy the output? As what? The original intent was probably to symlink it, or source it, or run it, depending on the original file name. Forcing preprocessing to be a handler means the preprocessor has to internally re-match the stripped filename against the rule system to figure out what to do next. That's handler logic inside a handler, and it compounds every time we add a new preprocessor.

    Making preprocessing a separate phase cleanly decomposes the problem. The preprocessor produces an output file; the rule system matches that output to a handler by its logical name; the handler does its normal job. Each piece has one responsibility. Adding a new preprocessor (secrets, plists, encrypted files) doesn't touch any handler code.

5. The Dodot Ethos Challenge

    Preprocessing puts pressure on two of dodot's hard constraints.

    - _Git is the source of truth._ If the deployed file and the source file differ, and the deployed file is what you actually edit, then `git diff` on the source no longer tells you what changed.
    - _No workflow change._ If the only way to propagate an edit back to git is to run `dodot render` or similar, that's an apply step by another name.

    Most dotfile managers that support templating or secrets accept these compromises. dodot tries harder. The mechanism is an optional git integration layer (clean and smudge filters, a pre-commit hook) that detects divergence and writes changes back to the source automatically at commit time, using each preprocessor's declared transform shape to decide how aggressive to be.

    The result: `git diff` shows the right thing at commit time, `git log` shows the right history, and PR reviews see real changes. The user accepts one Y/n per machine to install the git-side hooks; after that, the magic is ambient. The full design lives in [./../proposals/magic.lex].

6. What's Implemented, What's Planned

    - **Template preprocessing is implemented.** Write `config.toml.tmpl`, it renders on `dodot up` via MiniJinja, downstream handlers deploy the output. See [./../user/templates.lex] for usage. The preprocessing pipeline itself (the phase that turns source files into rendered outputs and feeds them into handlers) is shipped — templates are its first user.
    - **Plist support is also implemented**, but does NOT go through this preprocessing pipeline. It ships as a pair of git clean/smudge filters that translate macOS `*.plist` files between binary (working tree) and canonical XML (git index) on the fly. The architectural reasoning — why plists ducked out of the pipeline despite being a textbook Representational transform — is in [./../proposals/plists.lex] §2.3, and the user-facing reference is at [./plists.lex]. Shipped: `dodot plist clean/smudge` (the conversion engine), `dodot git-install-filters/show-filters` (the per-clone setup), `dodot prompts list/reset` (a generic dismissed-prompt registry that powers the up-time install offer).
    - **The git-integration layer for templates is shipped.** A per-file baseline cache (`rendered_hash`, `source_hash`, `context_hash`, `rendered_content`, `tracked_render`) sits under `<cache_dir>/preprocessor/`; `dodot transform check [--strict] [--dry-run]` runs the 4-state divergence matrix and applies reverse-merge diffs back to source via [burgertocow](https://crates.io/crates/burgertocow-lib) + [diffy](https://crates.io/crates/diffy); `dodot transform install-hook` registers a pre-commit hook that runs `dodot refresh && dodot transform check --strict` to refuse commits with unresolved drift; `dodot template install-filter` registers a git clean filter (`dodot template clean --path %f`) that makes `git status` and `git diff` see deployed-side template edits between commits, with a fast path that avoids re-rendering (and thus re-triggering any secret-provider auth); `dodot refresh` copies deployed-side mtimes onto sources so git's stat-cache invalidates; `dodot git-install-alias` lays down a Tier 3 shell alias (`alias git='dodot refresh --quiet && command git'`) for users who want `git status` to always reflect the latest template state; `dodot transform status` is a passive read-only view of the divergence cache. End-to-end design lives in [./../proposals/magic.lex]; user-facing walkthrough is at [./template-magic.lex].
    - **Secret handling is implemented**, both shapes covered. **Value injection** uses a `{{ secret("scheme:reference") }}` MiniJinja function inside templates, dispatched through a `SecretProvider` trait with built-in providers for `pass` (password-store), `op` (1Password CLI), `bw` (Bitwarden CLI), `sops` (Mozilla SOPS), `keychain` (macOS Keychain via `security`), and `secret-tool` (freedesktop Secret Service). Resolved values are cached within a single `dodot up` run; multi-line returns are refused at render time (whole-file is a separate path). **Whole-file decryption** uses `*.age` and `*.gpg` Opaque preprocessors that decrypt at deploy time and chmod the rendered datastore file to 0600 atomically. Both shapes share a per-render `<baseline>.secret.json` sidecar that lists which lines came from which `secret(...)` call; the sidecar is read at `dodot transform check` and clean-filter time so a rotated secret value in the deployed file doesn't get rewritten back into the template source as a literal. `dodot secret probe` and `dodot secret list` provide read-only inspection. Design and rationale: [./../proposals/shipped/secrets.lex] (preserved as historical context). User-facing guide: [./../user/secrets.lex]. Developer guide: [./../dev/secret.lex].

7. Where Rendered Output Lives

    Preprocessor output is written under the pack's datastore entry, in a `preprocessed/` subdirectory that mirrors the original pack layout.

    Rendered output path:

        ~/.local/share/dodot/packs/<pack>/preprocessed/<stripped-name>

    :: text ::

    The downstream handler creates its links or runs its commands against this path, not against the original source. This has two consequences worth knowing: the rendered file is what you inspect if you want to see exactly what a preprocessor produced; and for code-execution handlers (install, homebrew), the sentinel hash is derived from the rendered content, so changing a template variable re-triggers the install step on the next deploy.

    `dodot up` will not overwrite a rendered file whose bytes have diverged from the cached baseline (i.e. you've edited the deployed file in place since the last `up`). The render is skipped, the user's edits stay on disk, and a one-line warning surfaces. Resolve via `dodot transform check` (auto-merge the deployed-side edit back into the source) or re-run with `--force` (overwrite). Staleness is defined from file content only — env vars referenced via `{{ env.X }}` are read live at render time and are intentionally not part of the cache-invalidation signal; users who rotate a referenced env var pick up the new value with `dodot up --force`. See [./../proposals/preprocessing-pipeline.lex] §6.4.

8. Disabling and Overriding

    Preprocessing is on by default for any file matching a registered preprocessor's pattern. Four levels of opt-out:

    - Global: `[preprocessor] enabled = false` in the root `.dodot.toml`. All preprocessing is skipped; `.tmpl` files deploy verbatim.
    - Per preprocessor: `[preprocessor.template] enabled = false` disables template rendering but leaves other preprocessors active.
    - Per file (skip rendering entirely): add the filename to `[mappings] skip` and the file is ignored entirely.
    - Per file (skip reverse-merge only): add a glob pattern to `[preprocessor.template] no_reverse = ["..."]`. The template still renders on `dodot up`, but `dodot transform check` and the clean filter both skip reverse-merge for matching files. Useful for templates whose content is mostly dynamic (more `{{ }}` than static text) — burgertocow's heuristic degrades on those, so the user often gets more conflict markers than usable diffs. With `no_reverse` set, divergence is still detected and surfaced; only the auto-merge step is skipped.

    Pack-level `.dodot.toml` overrides the root for any of these keys, so you can flip a setting on or off for a single pack.
