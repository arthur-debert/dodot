:: verified ::
dodot plist

The "binary↔XML plist translator" command family. Two stdin → stdout subcommands that git invokes via the dodot-plist clean/smudge filter:

- `dodot plist clean` — binary plist on stdin → canonical XML plist on stdout. The clean direction (working tree → git index).
- `dodot plist smudge` — XML plist on stdin → binary plist on stdout. The smudge direction (git index → working tree).

You don't normally invoke these by hand. They're wired into git via [./git-install-filters.lex] (which writes the `[filter "dodot-plist"]` block) plus a `*.plist filter=dodot-plist` line in `.gitattributes`. See [./git-augmentation.lex] for the conceptual map.

1. When you reach for it (manually)

    - Diagnosing a "git status shows binary garbage on a plist" surprise — running `dodot plist clean < some.plist` by hand confirms whether the filter binary itself works.
    - One-off conversion of a plist to canonical XML for inspection or for sharing with a teammate.
    - Pre-flighting plist contents through dodot's normaliser before committing.

2. plist clean — binary → canonical XML

    Reads any plist on stdin (binary or XML) and emits canonical XML on stdout: dictionary keys sorted recursively, byte-stable formatting. This is the form git stores in the index.

    The canonicalisation is what makes diffs useful — without it, two semantically-equivalent plists with keys in different orders would diff as wholly-different content.

    Manual examples:

        dodot plist clean < ~/Library/Preferences/com.example.plist > /tmp/canonical.xml
        plutil -convert binary1 -o - foo.xml | dodot plist clean

    :: shell ::

3. plist smudge — XML → binary

    Reads XML on stdin and emits a binary plist on stdout. This is the form macOS apps actually read at runtime — `cfprefsd` and most apps prefer (or require) binary.

    Manual examples:

        dodot plist smudge < /tmp/canonical.xml > ~/Library/Preferences/com.example.plist

    :: shell ::

4. The git wiring

    The filter binding lives in two places:

    `.gitattributes` (committed with the repo):

        *.plist filter=dodot-plist

    :: text ::

    `.git/config` (per-clone, per-machine — written by [./git-install-filters.lex]):

        [filter "dodot-plist"]
            clean    = dodot plist clean
            smudge   = dodot plist smudge
            required = true

    :: text ::

    Both halves must be present for git to invoke the filter. The committed `.gitattributes` half travels with the repo; the per-clone `.git/config` half is what `git-install-filters` (or the install ladder during `up`) writes for you.

5. Watch out for

    - *macOS-acute, but not macOS-only.* The commands work on Linux too (binary plist parsing is platform-agnostic), but plists are macOS-native — you're unlikely to have any unless you're tracking macOS app config.
    - *cfprefsd cache.* After pulling plist changes from another machine, `cfprefsd` may keep serving stale values to running apps from its in-memory cache. dodot offers a `killall cfprefsd` prompt after `up` detects a plist change; you can also run that by hand.
    - *Symmetric round-trip is not guaranteed for ill-formed plists.* The canonical-XML form is well-defined; the binary form depends on Apple's encoder. Round-tripping a hand-edited XML through `smudge` then `clean` should produce identical XML, but starting from a malformed plist may surface errors at either step.
    - *`required = true` is intentional.* `git-install-filters` writes the filter with `required = true` so git refuses to silently skip the filter if `dodot` isn't on `$PATH`. The trade-off: a missing dodot binary breaks `git status` loudly, but recoverably. Removing `required` to "make git quieter" hides real failures.
