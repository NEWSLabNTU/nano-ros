# shellcheck shell=bash
# A path, as ONE bounded file-name component — issue 1927.
#
# Flattening a path into a single file name (`tr '/' '_'`) keeps it unique and
# gives it no length bound: the result is as long as the path, and Linux caps a
# component at NAME_MAX = 255 bytes. `mirror-generated-header.sh` named its run
# marker that way, so a build dir ~80 bytes deeper than `/tmp/…` (an agent
# session's TMPDIR, a CI workspace) failed with `File name too long`.
#
# nros_path_key <path>
#
# Prints `<tail>-<hash>`: <tail> is the last 48 bytes of the path with `/`
# flattened to `_` (so a listing still says which file the key is for), and
# <hash> is the first 16 hex digits of sha256(<path>), which is what makes the
# key UNIQUE per path — two paths sharing their last 48 bytes still differ in
# the hash. At most 48 + 1 + 16 = 65 bytes, whatever the input.
#
# The path is hashed as GIVEN, never resolved: a caller that wants two
# spellings of one file to share a key must canonicalise first. The mirror's
# callers pass the same absolute spelling every run (cmake writes it), and the
# gate asks the script for the key rather than recomputing it.
#
# Sourced from bash; no subshell beyond the hash itself.
nros_path_key() {
    local _p="${1:-}" _flat _tail _hash
    _flat="${_p//\//_}"
    _tail="${_flat: -48}"
    # `${x: -N}` yields the whole string when it is shorter than N.
    [ "${#_flat}" -lt 48 ] && _tail="$_flat"
    _tail="${_tail#_}"
    if command -v sha256sum >/dev/null 2>&1; then
        _hash="$(printf '%s' "$_p" | sha256sum)"
    else
        # BSD/macOS.
        _hash="$(printf '%s' "$_p" | shasum -a 256)"
    fi
    _hash="${_hash%% *}"
    printf '%s-%s\n' "$_tail" "${_hash:0:16}"
}
