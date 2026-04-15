#!/usr/bin/env bash
# Custom assertion helpers for dodot E2E tests.
#
# Mirror the assertion API from TempEnvironment in the Rust test suite.

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
    if ! grep -q "$pattern" "$path"; then
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
    matches=$(find "$sentinel_dir" -name "$pattern" -maxdepth 1 2>/dev/null)
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
