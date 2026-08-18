#!/usr/bin/env bats
# E2E acceptance for the shell hookup: INS01 (`dodot install`, spec §7)
# and RCS01 (the version-stamped footer and the hook-line trace).
#
# The story this file exists to prove, end to end in a clean
# environment: install dodot, `up` (the never-activated warning
# fires), `install --write` (a measured ✓), and a *real* new shell
# actually activates. Everything below runs the real binary against a
# real bash, because the whole point of this feature is that dodot
# stops taking the rc file's word for it — a mocked shell would test
# the mock.
#
# bash, not zsh: the CI image ships bash and may not ship zsh. `$SHELL`
# is exported per-test rather than in setup() so the refusal case can
# override it.

setup() {
    load helpers/setup
    sandbox_setup
    export SHELL=/bin/bash
    # The probe spawns `$SHELL -ic`, which for bash reads ~/.bashrc —
    # the same file the rc ladder targets. Nothing is there yet.
    rm -f "$HOME/.bashrc"
}

teardown() {
    sandbox_teardown
}

assert_no_verification_side_effects() {
    assert_not_exists "$XDG_DATA_HOME/dodot/probes/hookup/heartbeat"

    local profiles_dir="$XDG_DATA_HOME/dodot/probes/shell-init"
    if [ -d "$profiles_dir" ]; then
        local count
        count=$(find "$profiles_dir" -name 'profile-*.tsv' -type f | wc -l | tr -d ' ')
        if [ "$count" != "0" ]; then
            echo "expected 0 shell-init profiles, got $count" >&2
            return 1
        fi
    fi
}

append_slow_post_hook_work() {
    local rc="$1"
    cat >> "$rc" <<'SH'
touch "$HOME/post-hook-ran"
sleep 7
SH
}

prepend_slow_pre_hook_work() {
    local rc="$1"
    {
        printf '%s\n' 'sleep 7'
        cat "$rc"
    } > "$rc.new"
    mv "$rc.new" "$rc"
}

# ── The acceptance story (spec §7) ──────────────────────────────

@test "acceptance: up warns, install --write verifies, a new shell activates" {
    create_pack_file "shell" "aliases.sh" 'alias dodot_hookup_alias="echo activated"'
    create_pack_bin "tools" "dodot-hookup-tool" '#!/bin/sh
echo tool output'

    # 1. A green `up` on a machine with no hookup does not stay silent.
    run dodot up
    [ "$status" -eq 0 ]
    assert_output_contains "Shell hookup: a new shell did not load dodot."
    assert_output_contains "dodot install --write"

    # 2. `status` agrees, from evidence alone.
    run dodot status
    assert_output_contains "no shell has loaded dodot yet"

    # 3. Bare `install` reports and writes nothing.
    run dodot install
    [ "$status" -eq 0 ]
    assert_output_contains "nothing written yet"
    assert_not_exists "$HOME/.bashrc"

    # 4. `--write` wires it and ends on a *measured* verdict.
    run dodot install --write
    [ "$status" -eq 0 ]
    assert_output_contains "Shell hookup: dodot is sourced in new shells."
    assert_file_contains "$HOME/.bashrc" "# >>> dodot shell hookup >>>"
    assert_file_contains "$HOME/.bashrc" "dodot-init.sh"

    # 5. A real new shell activates: the alias and the bin/ entry are
    #    there because the rc was read, not because we sourced init by
    #    hand.
    run bash -ic 'dodot_hookup_alias'
    assert_output_contains "activated"

    run bash -ic 'command -v dodot-hookup-tool'
    [ "$status" -eq 0 ]

    # 6. And dodot now says so without spawning anything.
    run dodot status
    assert_output_contains "Shell hookup: dodot is sourced in new shells."
    assert_output_contains "Last loaded"
    assert_output_not_contains "no shell has loaded dodot yet"
}

# ── The properties that make the story safe to repeat ───────────

@test "bare install writes nothing at all" {
    dodot up
    echo "# my rc" > "$HOME/.bashrc"

    dodot install

    assert_file_contents "$HOME/.bashrc" "# my rc"
}

@test "install --write is idempotent" {
    dodot up
    dodot install --write
    local first
    first="$(cat "$HOME/.bashrc")"

    run dodot install --write
    assert_output_contains "already has dodot's hook block"

    [ "$(cat "$HOME/.bashrc")" = "$first" ]
    # One block, not two.
    [ "$(grep -c '>>> dodot shell hookup >>>' "$HOME/.bashrc")" -eq 1 ]
}

@test "install --write preserves the rest of the rc file" {
    dodot up
    printf 'export BEFORE=1\n' > "$HOME/.bashrc"

    dodot install --write

    assert_file_contains "$HOME/.bashrc" "export BEFORE=1"
    assert_file_contains "$HOME/.bashrc" "# >>> dodot shell hookup >>>"
}

@test "deleting the marked block is a complete uninstall" {
    create_pack_file "shell" "aliases.sh" 'alias dodot_hookup_alias="echo activated"'
    dodot up
    dodot install --write

    # Strip the block the way a user would.
    sed -i'' -e '/>>> dodot shell hookup >>>/,/<<< dodot shell hookup <<</d' \
        "$HOME/.bashrc"

    # `|| true` so the expected "command not found" is the assertion's
    # job, not a nonzero exit bats would warn about.
    run bash -ic 'dodot_hookup_alias || true'
    assert_output_not_contains "activated"

    run dodot install
    assert_output_contains "not found in that file"
}

@test "--rc overrides the file dodot would pick" {
    dodot up

    run dodot install --write --rc "$HOME/custom-rc"
    [ "$status" -eq 0 ]

    assert_file_contains "$HOME/custom-rc" "# >>> dodot shell hookup >>>"
    assert_not_exists "$HOME/.bashrc"
}

@test "a hand-wired eval hookup is reported, not claimed" {
    dodot up
    printf 'eval "$(dodot init-sh)"\n' > "$HOME/.bashrc"

    run dodot install
    assert_output_contains "wired by hand"
}

@test "an unsupported shell is refused with the line to paste" {
    dodot up

    SHELL=/bin/sh run dodot install
    [ "$status" -ne 0 ]
    assert_output_contains "will not guess an rc file"
    assert_output_contains "dodot-init.sh"
}

@test "a broken hookup is measured, not inferred from the rc file" {
    create_pack_file "shell" "aliases.sh" 'alias dodot_hookup_alias="echo activated"'
    dodot up
    dodot install --write

    # The hook is still in the file, but nothing reaches it. A static
    # scan would call this fine; only running the shell finds it.
    printf 'return 0\n%s\n' "$(cat "$HOME/.bashrc")" > "$HOME/.bashrc.new"
    mv "$HOME/.bashrc.new" "$HOME/.bashrc"
    rm -f "$XDG_DATA_HOME/dodot/probes/hookup/heartbeat"

    run dodot up
    assert_output_contains "Shell hookup: a new shell did not load dodot."
    assert_output_contains "never reached"
}

# ── Targeted verification after mutations (SIV01 WS02) ──────────

@test "up verifies bash at the hook without running pack contributions or post-hook rc work" {
    dodot install --write
    append_slow_post_hook_work "$HOME/.bashrc"
    create_pack_file "shell" "aliases.sh" 'touch "$HOME/pack-contribution-ran"
sleep 7'

    run dodot up
    [ "$status" -eq 0 ]
    assert_output_contains "Shell hookup: dodot is sourced in new shells."

    assert_not_exists "$HOME/post-hook-ran"
    assert_not_exists "$HOME/pack-contribution-ran"
    assert_no_verification_side_effects
}

@test "up verifies zsh at the hook without running post-hook rc work" {
    if ! command -v zsh >/dev/null 2>&1; then
        skip "zsh not installed on this host"
    fi
    export SHELL
    SHELL="$(command -v zsh)"

    dodot install --write
    append_slow_post_hook_work "$HOME/.zshrc"
    create_pack_file "shell" "aliases.sh" 'touch "$HOME/pack-contribution-ran"
sleep 7'

    run dodot up
    [ "$status" -eq 0 ]
    assert_output_contains "Shell hookup: dodot is sourced in new shells."

    assert_not_exists "$HOME/post-hook-ran"
    assert_not_exists "$HOME/pack-contribution-ran"
    assert_no_verification_side_effects
}

@test "install --write verifies the replaced bash block at the hook" {
    create_pack_file "shell" "aliases.sh" 'touch "$HOME/pack-contribution-ran"
sleep 7'
    dodot up
    cat > "$HOME/.bashrc" <<'SH'
# >>> dodot shell hookup >>>
[ -f "$HOME/old-dodot-init.sh" ] && . "$HOME/old-dodot-init.sh"
# <<< dodot shell hookup <<<
touch "$HOME/post-hook-ran"
sleep 7
SH

    run dodot install --write
    [ "$status" -eq 0 ]
    assert_output_contains "replaced"
    assert_output_contains "Shell hookup: dodot is sourced in new shells."

    assert_not_exists "$HOME/post-hook-ran"
    assert_not_exists "$HOME/pack-contribution-ran"
    assert_no_verification_side_effects
}

@test "up reports a bash timeout when work before the capable hook never yields evidence" {
    dodot install --write
    prepend_slow_pre_hook_work "$HOME/.bashrc"
    create_pack_file "shell" "aliases.sh" 'alias dodot_hookup_alias="echo activated"'

    run dodot up
    [ "$status" -eq 0 ]
    assert_output_contains "Shell hookup: a new shell did not load dodot."
    assert_output_contains "never reached"
    assert_no_verification_side_effects
}

# ── The version-stamped footer (RCS01 WS01) ─────────────────────

@test "the footer catches a heartbeat left by a different dodot version" {
    dodot up
    dodot install --write
    bash -ic true   # a real shell activates, writing the heartbeat

    # A shell whose rc resolves an older dodot would leave this exact
    # heartbeat: same generation (the script is current), another
    # version. Keep the real generation so only the version disagrees —
    # the epic's motivating failure, invisible before RCS01.
    local hb="$XDG_DATA_HOME/dodot/probes/hookup/heartbeat"
    read -r gen _ < "$hb"
    echo "$gen 5.0.0" > "$hb"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "Shell hookup: your shells load a different dodot."
    assert_output_contains "by dodot 5.0.0 — you are running"
}

@test "a version-less heartbeat renders as at most the pre-RCS01 release" {
    dodot up
    dodot install --write
    bash -ic true   # a real shell activates, writing the heartbeat

    # A pre-RCS01 init script wrote only the generation. One rule
    # covers it: the version renders as a bound, never as a guess.
    local hb="$XDG_DATA_HOME/dodot/probes/hookup/heartbeat"
    read -r gen _ < "$hb"
    echo "$gen" > "$hb"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "by dodot ≤5.5.1"
}

@test "after down, a shell that loads the empty script reads as wired-but-empty" {
    create_pack_file "shell" "aliases.sh" 'alias dodot_hookup_alias="echo activated"'
    dodot up
    dodot install --write
    dodot down

    # The hookup is still wired and firing — a real shell sources the
    # regenerated, contribution-less script — so a healthy line would
    # be technically true and practically misleading.
    bash -ic true

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "Shell hookup: wired, but no packs are deployed."
    assert_output_contains "Run \`dodot up\` to deploy your packs."
}

# ── The hook-line trace (`probe shell-init`, RCS01 WS02) ────────

@test "probe shell-init names the hook line and the stale binary a mangled PATH resolves to" {
    dodot up

    # A stale dodot the mangled PATH will resolve to instead of the
    # binary under test.
    mkdir -p "$HOME/stale-bin"
    printf '#!/bin/sh\necho "dodot 5.0.0"\n' > "$HOME/stale-bin/dodot"
    chmod +x "$HOME/stale-bin/dodot"

    # An rc that rebuilds PATH *before* the hand-wired hook — the
    # epic's motivating failure. The hook sits on line 2.
    printf 'export PATH=%s\neval "$(dodot init-sh)"\n' "$HOME/stale-bin" \
        > "$HOME/.bashrc"

    run dodot probe shell-init --trace-hook
    [ "$status" -eq 0 ]
    assert_output_contains "Hook resolution"
    # The right line…
    assert_output_contains ".bashrc:2"
    # …and the right binary, version included.
    assert_output_contains "different dodot"
    assert_output_contains "stale-bin/dodot"
    assert_output_contains "5.0.0"
}

_mtime_of() {
    stat -c %Y "$1" 2>/dev/null || stat -f %m "$1"
}

_profiles_snapshot() {
    local dir="$XDG_DATA_HOME/dodot/probes/shell-init"
    if [[ ! -d "$dir" ]]; then
        return 0
    fi
    while IFS= read -r file; do
        printf '%s ' "${file#$dir/}"
        cksum "$file"
    done < <(find "$dir" -type f -name 'profile-*' | sort)
}

@test "trace-hook current file-source target leaves evidence untouched and runs contributions" {
    create_pack_file "shell" "trace.sh" 'echo trace contribution > "$HOME/trace-contribution"'
    dodot up
    dodot install --write

    bash -ic true
    local hb="$XDG_DATA_HOME/dodot/probes/hookup/heartbeat"
    local hb_contents hb_mtime profiles_before
    hb_contents="$(cat "$hb")"
    hb_mtime="$(_mtime_of "$hb")"
    profiles_before="$(_profiles_snapshot)"
    rm -f "$HOME/trace-contribution"

    run dodot probe shell-init --trace-hook
    [ "$status" -eq 0 ]
    assert_output_contains "Hook resolution"
    assert_output_contains "the hookup is sound"
    assert_output_contains "elapsed:"

    assert_exists "$HOME/trace-contribution"
    [ "$(cat "$hb")" = "$hb_contents" ]
    [ "$(_mtime_of "$hb")" = "$hb_mtime" ]
    [ "$(_profiles_snapshot)" = "$profiles_before" ]
}

@test "trace-hook current eval target leaves evidence untouched and runs contributions" {
    create_pack_file "shell" "trace.sh" 'echo trace contribution > "$HOME/trace-contribution"'
    dodot up
    printf 'export PATH=%s:$PATH\neval "$(dodot init-sh)"\n' "$(dirname "$DODOT_BIN")" \
        > "$HOME/.bashrc"

    bash -ic true
    local hb="$XDG_DATA_HOME/dodot/probes/hookup/heartbeat"
    local hb_contents hb_mtime profiles_before
    hb_contents="$(cat "$hb")"
    hb_mtime="$(_mtime_of "$hb")"
    profiles_before="$(_profiles_snapshot)"
    rm -f "$HOME/trace-contribution"

    run dodot probe shell-init --trace-hook
    [ "$status" -eq 0 ]
    assert_output_contains "Hook resolution"
    assert_output_contains "the hookup is sound"
    assert_output_contains "elapsed:"

    assert_exists "$HOME/trace-contribution"
    [ "$(cat "$hb")" = "$hb_contents" ]
    [ "$(_mtime_of "$hb")" = "$hb_mtime" ]
    [ "$(_profiles_snapshot)" = "$profiles_before" ]
}

@test "trace-hook capability-unknown eval target uses copy and exits before the hook" {
    dodot up
    printf 'echo should-not-run > "$HOME/eval-hook-ran"; eval "$(dodot init-sh)"\n' \
        > "$HOME/.bashrc"

    run dodot probe shell-init --trace-hook
    [ "$status" -eq 0 ]
    assert_output_contains "Hook resolution"
    assert_output_contains "temporary copy"
    assert_output_contains "elapsed:"
    assert_not_exists "$HOME/eval-hook-ran"
    assert_not_exists "$XDG_DATA_HOME/dodot/probes/hookup/heartbeat"
    [ -z "$(_profiles_snapshot)" ]
}

@test "probe shell-init --no-trace keeps the report passive" {
    dodot up
    printf 'eval "$(dodot init-sh)"\n' > "$HOME/.bashrc"

    run dodot probe shell-init --no-trace
    [ "$status" -eq 0 ]
    assert_output_not_contains "Hook resolution"
    assert_output_not_contains "tracing shell startup"
}

@test "a file argument keeps probe shell-init passive too" {
    # The shipped status warning points users at exactly this
    # invocation; it must stay cheap and spawn nothing.
    dodot up
    printf 'eval "$(dodot init-sh)"\n' > "$HOME/.bashrc"

    run dodot probe shell-init gpg/env.sh
    [ "$status" -eq 0 ]
    assert_output_not_contains "Hook resolution"
    assert_output_not_contains "tracing shell startup"
}

# The file-source half of the same rule: the verdict is about the
# script the hook line names, not the one `dodot up` writes. Both
# exist here — the datastore's script is perfectly intact — and the
# hook sources a copy that is not there. Judging the expected path
# would report this dead hookup as sound.
@test "probe shell-init judges the script the hook line sources, not the one up wrote" {
    dodot up
    # `up` wrote this one, and it is fine.
    [ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ]

    # The hook sources a copy that does not exist.
    printf '[ -f "$HOME/old/dodot-init.sh" ] && . "$HOME/old/dodot-init.sh"\n' \
        > "$HOME/.bashrc"

    run dodot probe shell-init --trace-hook
    [ "$status" -eq 0 ]
    assert_output_contains ".bashrc:1"
    assert_output_contains "does not exist"
    assert_output_contains "old/dodot-init.sh"
    assert_output_not_contains "the hookup is sound"
}

# The same rule one level deeper: the script is *there*, and it is a
# real dodot init script — written by an older dodot. Every filesystem
# fact says "present"; the hookup is the epic's motivating failure.
@test "probe shell-init reports a foreign init script as skew, not as sound" {
    dodot up

    # A 5.0.0 init script, stamped the way that dodot stamped them.
    mkdir -p "$HOME/old"
    printf '# dodot init\nexport DODOT_INIT_GEN=1\nexport DODOT_INIT_VERSION=5.0.0\n' \
        > "$HOME/old/dodot-init.sh"
    printf '[ -f "$HOME/old/dodot-init.sh" ] && . "$HOME/old/dodot-init.sh"\n' \
        > "$HOME/.bashrc"

    run dodot probe shell-init --trace-hook
    [ "$status" -eq 0 ]
    assert_output_contains "written by a different dodot"
    assert_output_contains "5.0.0"
    assert_output_not_contains "the hookup is sound"
}

# A hook line dodot cannot resolve without running a shell gets an
# honest non-answer. The alternative is a verdict about whichever file
# dodot guessed at, which is the failure mode this epic is about.
@test "probe shell-init declines to guess at a hook line it cannot resolve" {
    dodot up
    printf '. "$XDG_DATA_HOME/dodot/shell/dodot-init.sh"\n' > "$HOME/.bashrc"

    run dodot probe shell-init
    [ "$status" -eq 0 ]
    assert_output_contains "could not tell which file the hook"
    assert_output_not_contains "the hookup is sound"
}

@test "probe shell-init reports an absent hook plainly and spawns nothing" {
    dodot up
    printf 'alias ll="ls -l"\n' > "$HOME/.bashrc"

    run dodot probe shell-init
    [ "$status" -eq 0 ]
    assert_output_contains "no dodot hook"
    assert_output_not_contains "tracing shell startup"
}
