#!/usr/bin/env bats
# E2E tests for the conditional-running (gates) feature.
#
# Covers the five surfaces from `docs/user/conditional-running.lex`:
# filename suffix `._<label>`, directory segment `_<label>/`,
# `[pack] os` whole-pack gating, `[mappings.gates]` glob escape
# hatch, and the user-defined `[gates]` table.
#
# Each test pins the gated label to the *current* host's OS or to a
# never-matching string, so tests are portable across darwin and
# linux CI without `target_os` cfg gates.

setup() {
    load helpers/setup
    sandbox_setup

    # Detect the canonical gate OS name for the current host so each
    # test can choose "match" vs "no-match" labels portably. dodot
    # exposes this via the `dodot.os` template var, but for bats we
    # just inspect `uname` and map to dodot's canonical names.
    case "$(uname -s)" in
        Darwin) HOST_OS="darwin"; OTHER_OS="linux" ;;
        Linux)  HOST_OS="linux";  OTHER_OS="darwin" ;;
        *) skip "unsupported host OS: $(uname -s)" ;;
    esac
    export HOST_OS OTHER_OS
}

teardown() {
    sandbox_teardown
}

# ── Filename-suffix gates ───────────────────────────────────────

@test "passing filename gate deploys under stripped name" {
    create_pack_file "vim" "home.vimrc._${HOST_OS}" "set nocompatible"

    dodot up

    # User-side link lands at ~/.vimrc — the gate suffix is stripped
    # for routing (so home.X resolves to $HOME/.vimrc), but the
    # datastore data-link keeps the original filename to match what's
    # on disk in the pack.
    [ -L "$HOME/.vimrc" ]
    local target
    target="$(readlink "$HOME/.vimrc")"
    [[ "$target" == *"/dodot/packs/vim/symlink/home.vimrc._${HOST_OS}" ]]
    # And the contents resolve through the chain.
    [ "$(cat "$HOME/.vimrc")" = "set nocompatible" ]
}

@test "failing filename gate does not deploy" {
    create_pack_file "vim" "home.vimrc._${OTHER_OS}" "set nocompatible"

    dodot up

    assert_not_exists "$HOME/.vimrc"
    assert_no_handler_state "vim" "symlink"
}

@test "status surfaces failing filename gate as gated out" {
    create_pack_file "vim" "home.vimrc._${OTHER_OS}" "x"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "home.vimrc._${OTHER_OS}"
    assert_output_contains "gated out (${OTHER_OS})"
}

@test "passing and failing gates coexist in one pack" {
    create_pack_file "tools" "install._${HOST_OS}.sh"  "echo run"
    create_pack_file "tools" "install._${OTHER_OS}.sh" "echo skip"
    create_pack_file "tools" "vimrc"                   "set nocompatible"

    dodot up

    # The matching install ran (sentinel exists); the non-matching one
    # did not (no sentinel for `install._<other>.sh-*`). Both surface
    # in `dodot status` either way — one as run, one as gated out.
    assert_sentinel_exists "tools" "install" "install.sh-*"
    run dodot status
    assert_output_contains "gated out (${OTHER_OS})"
}

# ── Directory-segment gates ─────────────────────────────────────

@test "passing directory-segment gate flattens contents at pack root" {
    create_pack_file "cross" "_${HOST_OS}/home.vimrc" "set nocompatible"

    dodot up

    # The `_<host>/` segment vanishes; `home.vimrc` surfaces at pack
    # root and routes via the home.X prefix to ~/.vimrc.
    assert_symlink "$HOME/.vimrc" \
        "$XDG_DATA_HOME/dodot/packs/cross/symlink/home.vimrc"
}

@test "failing directory-segment gate surfaces as gated dir in status" {
    create_pack_file "cross" "_${OTHER_OS}/home.vimrc" "set nocompatible"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "_${OTHER_OS}"
    assert_output_contains "gated out (${OTHER_OS})"

    # Nothing inside the gated dir reaches the deploy layer.
    assert_not_exists "$HOME/.vimrc"
}

# ── Pack-level [pack] os ────────────────────────────────────────

@test "[pack] os matching deploys the pack" {
    create_pack_file   "active" "vimrc" "set nocompatible"
    create_pack_config "active" "[pack]\nos = [\"${HOST_OS}\"]\n"

    dodot up

    # vimrc deploys under XDG default routing ($XDG_CONFIG_HOME/active/vimrc).
    assert_symlink "$XDG_CONFIG_HOME/active/vimrc" \
        "$XDG_DATA_HOME/dodot/packs/active/symlink/vimrc"
}

@test "[pack] os mismatch deactivates the pack and surfaces in status" {
    create_pack_file   "inactive" "vimrc" "x"
    create_pack_config "inactive" "[pack]\nos = [\"${OTHER_OS}\"]\n"

    dodot up
    assert_not_exists "$XDG_CONFIG_HOME/inactive/vimrc"
    assert_no_handler_state "inactive" "symlink"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "Inactive on this OS"
    assert_output_contains "inactive"
    assert_output_contains "current=${HOST_OS}"
}

@test "[pack] os macos alias matches darwin" {
    if [ "$HOST_OS" != "darwin" ]; then
        skip "alias test only meaningful on darwin"
    fi

    create_pack_file   "mac" "vimrc" "x"
    create_pack_config "mac" "[pack]\nos = [\"macos\"]\n"

    dodot up
    assert_symlink "$XDG_CONFIG_HOME/mac/vimrc" \
        "$XDG_DATA_HOME/dodot/packs/mac/symlink/vimrc"
}

@test "root-level [pack] os is rejected" {
    create_pack_file "vim" "vimrc" "x"
    create_root_config '[pack]\nos = ["darwin"]\n'

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "root-level"
    assert_output_contains "[pack] os"
}

# ── User-defined [gates] labels ─────────────────────────────────

@test "user-defined compound label: passing predicate deploys file" {
    # Define a compound label keyed off the current host's actual
    # os+arch — this exercises the AND-of-equalities path with a
    # predicate guaranteed to match. macOS `uname -m` reports
    # `arm64` while Rust's `target_arch` is `aarch64`; map between
    # them so the predicate aligns with what `HostFacts` carries.
    local host_arch
    case "$(uname -m)" in
        arm64) host_arch="aarch64" ;;
        *)     host_arch="$(uname -m)" ;;
    esac
    local pack="custom"
    create_root_config "$(printf '[gates]\n"this-host" = { os = "%s", arch = "%s" }\n' \
        "${HOST_OS}" "${host_arch}")"

    create_pack_file "$pack" "vimrc._this-host" "x"

    dodot up

    # Predicate matches → file deploys under stripped XDG path; the
    # datastore data-link keeps the gated source filename.
    [ -L "$XDG_CONFIG_HOME/$pack/vimrc" ]
    local target
    target="$(readlink "$XDG_CONFIG_HOME/$pack/vimrc")"
    [[ "$target" == *"/dodot/packs/$pack/symlink/vimrc._this-host" ]]
}

@test "user-defined compound label: failing predicate gates file out" {
    # Compound label that can never match the live host: same os as
    # the host, but an arch value Rust target_arch never produces.
    # Exercises the gate-fail path for valid user-defined labels —
    # paired with the passing-predicate test above so both sides of
    # the predicate are covered.
    local pack="custom"
    create_root_config "$(printf '[gates]\n"impossible-host" = { os = "%s", arch = "imaginary-cpu" }\n' \
        "${HOST_OS}")"

    create_pack_file "$pack" "vimrc._impossible-host" "x"

    dodot up

    # Predicate fails → file does not deploy; surfaces in status as
    # gated out under its original (unstripped) name.
    assert_not_exists "$XDG_CONFIG_HOME/$pack/vimrc"
    assert_no_handler_state "$pack" "symlink"

    run dodot status
    [ "$status" -eq 0 ]
    assert_output_contains "vimrc._impossible-host"
    assert_output_contains "gated out (impossible-host)"
}

@test "unknown gate label hard-errors at scan time" {
    create_pack_file "typo" "install._darwn.sh" "echo nope"

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "unknown gate label"
    assert_output_contains "darwn"
}

# ── [mappings.gates] glob escape hatch ──────────────────────────

@test "[mappings.gates] passing glob deploys file" {
    create_pack_file   "legacy" "install-host.sh" "echo run"
    create_pack_config "legacy" "$(printf '[mappings.gates]\n"install-host.sh" = "%s"\n' "${HOST_OS}")"

    dodot up

    # Filename doesn't match install handler's default patterns, so
    # the catchall symlink claims it. Existence proves the gate
    # passed and dispatch reached the catchall.
    assert_symlink "$XDG_CONFIG_HOME/legacy/install-host.sh" \
        "$XDG_DATA_HOME/dodot/packs/legacy/symlink/install-host.sh"
}

@test "[mappings.gates] failing glob skips file" {
    create_pack_file   "legacy" "install-other.sh" "x"
    create_pack_config "legacy" "$(printf '[mappings.gates]\n"install-other.sh" = "%s"\n' "${OTHER_OS}")"

    dodot up

    assert_not_exists "$XDG_CONFIG_HOME/legacy/install-other.sh"

    run dodot status
    assert_output_contains "gated out (${OTHER_OS})"
}

@test "[mappings.gates] conflict with filename gate is rejected" {
    create_pack_file   "conflict" "install._${HOST_OS}.sh" "x"
    create_pack_config "conflict" "$(printf '[mappings.gates]\n"install._%s.sh" = "%s"\n' "${HOST_OS}" "${OTHER_OS}")"

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "gate-routing conflict"
}

@test "invalid [mappings.gates] glob is hard error" {
    create_pack_file   "broken" "vimrc" "x"
    create_pack_config "broken" '[mappings.gates]\n"[unclosed" = "darwin"\n'

    run dodot up
    [ "$status" -ne 0 ]
    assert_output_contains "invalid"
    assert_output_contains "[mappings.gates]"
}

# ── adopt --only-os ─────────────────────────────────────────────

@test "adopt --only-os wraps adopted file in gate dir" {
    # Need an existing pack to adopt into. Use ~/.vimrc rather than
    # ~/.bashrc because `bashrc` is in `force_home` and would adopt
    # to a bare name (`bashrc`) — vimrc takes the `home.X` per-file
    # routing prefix, which gives a more interesting round-trip.
    create_pack_file "vim" "placeholder" ""
    create_home_file ".vimrc" "set nocompatible"

    run dodot adopt "$HOME/.vimrc" --only-os "${HOST_OS}" --into vim
    [ "$status" -eq 0 ]

    # The adopted entry lands at <pack>/_<label>/home.vimrc — gate
    # dir wraps the routing-prefix file so re-deploy on a matching
    # host strips the gate dir and routes through home.X to ~/.vimrc.
    assert_file_contents \
        "$DOTFILES_ROOT/vim/_${HOST_OS}/home.vimrc" "set nocompatible"

    # Original is now a symlink (adopt's standard contract).
    [ -L "$HOME/.vimrc" ]

    # Re-deploying on the same host puts the file back where it came
    # from — the round-trip the proposal §11 promises.
    dodot up
    [ -L "$HOME/.vimrc" ]
    # Read through the chain — content must round-trip even on
    # macOS where readlink -f canonicalises /tmp to /private/tmp.
    [ "$(cat "$HOME/.vimrc")" = "set nocompatible" ]
}

@test "adopt --only-os rejects unknown labels before any FS work" {
    create_pack_file "vim" "placeholder" ""
    create_home_file ".vimrc" "x"

    run dodot adopt "$HOME/.vimrc" --only-os "nonexistent-label" --into vim
    [ "$status" -ne 0 ]
    assert_output_contains "unknown gate label"
    assert_output_contains "--only-os"

    # Source file untouched.
    [ -f "$HOME/.vimrc" ]
    [ ! -L "$HOME/.vimrc" ]
    # Pack didn't get a gate dir.
    [ ! -d "$DOTFILES_ROOT/vim/_nonexistent-label" ]
}
