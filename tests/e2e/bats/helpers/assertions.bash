#!/usr/bin/env bash
# Custom assertion helpers for dodot E2E tests.
#
# Mirror the assertion API from TempEnvironment in the Rust test suite.
#
# ## Instrumented assertions
#
# These work with fixtures created by instrumented_* helpers in fixtures.bash.
# They check env vars, stdout markers, marker files, and brew mock logs
# that instrumented fixtures produce when loaded/executed.

# Assert that a symlink exists and points to the expected target.
# Usage: assert_symlink "/path/to/link" "/path/to/target"
assert_symlink() {
    local link="$1"
    local target="$2"

    if [[ ! -L "$link" ]]; then
        echo "expected symlink at $link, but it is not a symlink" >&2
        if [[ -e "$link" ]]; then
            echo "  (exists as regular file/dir)" >&2
        else
            echo "  (does not exist)" >&2
        fi
        return 1
    fi

    local actual_target
    actual_target="$(readlink "$link")"
    if [[ "$actual_target" != "$target" ]]; then
        echo "symlink $link points to $actual_target, expected $target" >&2
        return 1
    fi
}

# Assert the full dodot double-link chain:
#   source (pack file) -> datastore link -> user-visible path
#
# Usage: assert_double_link "vim" "symlink" "vimrc" "/path/to/source" "/path/to/user/file"
assert_double_link() {
    local pack="$1"
    local handler="$2"
    local filename="$3"
    local source="$4"
    local user_path="$5"

    local datastore_link="$XDG_DATA_HOME/dodot/packs/$pack/$handler/$filename"

    # Datastore link -> source
    assert_symlink "$datastore_link" "$source"

    # User link -> datastore link
    assert_symlink "$user_path" "$datastore_link"
}

# Assert that a file exists.
# Usage: assert_exists "/path/to/file"
assert_exists() {
    local path="$1"
    if [[ ! -e "$path" ]]; then
        echo "expected $path to exist, but it does not" >&2
        return 1
    fi
}

# Assert that nothing exists at the given path.
# Usage: assert_not_exists "/path/to/file"
assert_not_exists() {
    local path="$1"
    if [[ -e "$path" || -L "$path" ]]; then
        echo "expected $path to not exist, but it does" >&2
        return 1
    fi
}

# Assert that a directory exists.
# Usage: assert_dir_exists "/path/to/dir"
assert_dir_exists() {
    local path="$1"
    if [[ ! -d "$path" ]]; then
        echo "expected $path to be a directory" >&2
        return 1
    fi
}

# Assert a file exists and contains a pattern.
# Usage: assert_file_contains "/path/to/file" "pattern"
assert_file_contains() {
    local path="$1"
    local pattern="$2"

    assert_exists "$path"
    if ! grep -q -e "$pattern" "$path"; then
        echo "expected $path to contain '$pattern'" >&2
        echo "actual contents:" >&2
        cat "$path" >&2
        return 1
    fi
}

# Assert a file exists with exactly the given contents.
# Usage: assert_file_contents "/path/to/file" "expected content"
assert_file_contents() {
    local path="$1"
    local expected="$2"

    assert_exists "$path"
    local actual
    actual="$(cat "$path")"
    if [[ "$actual" != "$expected" ]]; then
        echo "file $path has unexpected contents" >&2
        echo "expected: $expected" >&2
        echo "actual:   $actual" >&2
        return 1
    fi
}

# Assert that a sentinel file matching a glob pattern exists.
# Usage: assert_sentinel_exists "tools" "install" "install.sh-*"
assert_sentinel_exists() {
    local pack="$1"
    local handler="$2"
    local pattern="$3"

    local sentinel_dir="$XDG_DATA_HOME/dodot/packs/$pack/$handler"
    if [[ ! -d "$sentinel_dir" ]]; then
        echo "sentinel dir $sentinel_dir does not exist" >&2
        return 1
    fi

    # shellcheck disable=SC2086
    local matches
    matches=$(find "$sentinel_dir" -maxdepth 1 -name "$pattern" 2>/dev/null)
    if [[ -z "$matches" ]]; then
        echo "no sentinel matching '$pattern' in $sentinel_dir" >&2
        echo "contents:" >&2
        ls -la "$sentinel_dir" >&2
        return 1
    fi
}

# Assert no handler state exists for a pack/handler pair.
# Usage: assert_no_handler_state "vim" "symlink"
assert_no_handler_state() {
    local pack="$1"
    local handler="$2"
    local dir="$XDG_DATA_HOME/dodot/packs/$pack/$handler"

    if [[ -d "$dir" ]]; then
        local count
        count=$(find "$dir" -mindepth 1 -maxdepth 1 | wc -l)
        if [[ "$count" -gt 0 ]]; then
            echo "expected no state for $pack/$handler, but found $count entries in $dir" >&2
            ls -la "$dir" >&2
            return 1
        fi
    fi
}

# Assert that dodot output (from $output) contains a string.
# Usage: run dodot status
#        assert_output_contains "vim"
assert_output_contains() {
    local pattern="$1"
    if [[ "$output" != *"$pattern"* ]]; then
        echo "expected output to contain '$pattern'" >&2
        echo "actual output:" >&2
        echo "$output" >&2
        return 1
    fi
}

# Assert that dodot output does NOT contain a string.
# Usage: run dodot status
#        assert_output_not_contains "error"
assert_output_not_contains() {
    local pattern="$1"
    if [[ "$output" == *"$pattern"* ]]; then
        echo "expected output to NOT contain '$pattern'" >&2
        echo "actual output:" >&2
        echo "$output" >&2
        return 1
    fi
}

# ── Instrumented assertions ─────────────────────────────────────
#
# These require eval_init_sh to have been called first (for shell/path),
# or dodot up to have run (for install/brew).

# Assert that an instrumented shell file was loaded via init-sh.
# Checks the DODOT_LOADED_{PACK}_{FILE} env var is set.
# Usage: dodot up && eval_init_sh
#        assert_shell_loaded "vim" "aliases.sh"
assert_shell_loaded() {
    local pack="$1"
    local filename="$2"
    local var_name="DODOT_LOADED_$(_normalize "$pack" "$filename")"

    local val="${!var_name:-}"
    if [[ "$val" != "1" ]]; then
        echo "expected $var_name=1 (shell file $pack/$filename loaded)" >&2
        echo "  actual: ${var_name}=${val:-<unset>}" >&2
        echo "  hint: did you call eval_init_sh after dodot up?" >&2
        return 1
    fi
}

# Assert that an instrumented shell file was NOT loaded.
# Usage: assert_shell_not_loaded "vim" "aliases.sh"
assert_shell_not_loaded() {
    local pack="$1"
    local filename="$2"
    local var_name="DODOT_LOADED_$(_normalize "$pack" "$filename")"

    local val="${!var_name:-}"
    if [[ "$val" == "1" ]]; then
        echo "expected $var_name to be unset (shell file $pack/$filename should not be loaded)" >&2
        return 1
    fi
}

# Assert that an instrumented bin script is callable and produces the expected marker.
# Usage: dodot up && eval_init_sh
#        assert_bin_available "tools" "devtool"
assert_bin_available() {
    local pack="$1"
    local script_name="$2"
    local marker="DODOT_BIN_$(_normalize "$pack" "$script_name")"

    if ! command -v "$script_name" >/dev/null 2>&1; then
        echo "expected '$script_name' to be on PATH" >&2
        echo "  PATH=$PATH" >&2
        echo "  hint: did you call eval_init_sh after dodot up?" >&2
        return 1
    fi

    local actual
    actual="$("$script_name" 2>&1)"
    if [[ "$actual" != *"$marker"* ]]; then
        echo "expected running '$script_name' to output '$marker'" >&2
        echo "  actual output: $actual" >&2
        return 1
    fi
}

# Assert that an instrumented bin script is NOT on PATH.
# Usage: assert_bin_not_available "tools" "devtool"
assert_bin_not_available() {
    local script_name="$2"

    if command -v "$script_name" >/dev/null 2>&1; then
        echo "expected '$script_name' to NOT be on PATH, but found: $(command -v "$script_name")" >&2
        return 1
    fi
}

# Assert that an instrumented install script has run.
# Checks marker file at $HOME/.dodot-markers/{pack}.install.
# Usage: dodot up
#        assert_install_ran "tools"
assert_install_ran() {
    local pack="$1"
    local marker="$HOME/.dodot-markers/${pack}.install"

    if [[ ! -f "$marker" ]]; then
        echo "expected install marker at $marker (install.sh for pack '$pack' should have run)" >&2
        echo "  contents of $HOME/.dodot-markers/:" >&2
        ls -la "$HOME/.dodot-markers/" 2>&1 >&2 || echo "  (directory does not exist)" >&2
        return 1
    fi
}

# Assert that an instrumented install script has NOT run.
# Usage: assert_install_not_ran "tools"
assert_install_not_ran() {
    local pack="$1"
    local marker="$HOME/.dodot-markers/${pack}.install"

    if [[ -f "$marker" ]]; then
        echo "expected no install marker at $marker (install.sh for pack '$pack' should NOT have run)" >&2
        return 1
    fi
}

# Assert that the brew mock was invoked.
# Usage: install_brew_mock; dodot up
#        assert_brew_invoked
assert_brew_invoked() {
    local log="$HOME/.dodot-markers/brew.log"

    if [[ ! -f "$log" ]]; then
        echo "expected brew mock log at $log, but it does not exist" >&2
        echo "  hint: did you call install_brew_mock before dodot up?" >&2
        return 1
    fi
}

# Assert that the brew mock was invoked with specific arguments.
# Usage: assert_brew_invoked_with "bundle" "--file="
assert_brew_invoked_with() {
    local log="$HOME/.dodot-markers/brew.log"

    assert_brew_invoked

    for pattern in "$@"; do
        if ! grep -q -F -- "$pattern" "$log"; then
            echo "expected brew log to contain '$pattern'" >&2
            echo "  actual log:" >&2
            cat "$log" >&2
            return 1
        fi
    done
}

# Assert that the brew mock was NOT invoked.
# Usage: assert_brew_not_invoked
assert_brew_not_invoked() {
    local log="$HOME/.dodot-markers/brew.log"

    if [[ -f "$log" ]]; then
        echo "expected brew mock log to not exist, but found:" >&2
        cat "$log" >&2
        return 1
    fi
}

# Assert that a specific env var is set to a specific value.
# Useful for checking custom exports from shell fixtures.
# Usage: assert_env_var "ZSH_PROFILE_LOADED" "1"
assert_env_var() {
    local var_name="$1"
    local expected="$2"

    local val="${!var_name:-}"
    if [[ "$val" != "$expected" ]]; then
        echo "expected $var_name='$expected'" >&2
        echo "  actual: ${var_name}='${val:-<unset>}'" >&2
        return 1
    fi
}
