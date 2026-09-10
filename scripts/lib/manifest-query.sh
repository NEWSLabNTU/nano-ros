# shellcheck shell=bash
# Sourced, not executed — hence no shebang.
#
# nros_manifest_query — run `fixtures-manifest.py` and tell "parsed cleanly,
# printed nothing" apart from "failed to parse" (issue 1264).
#
# Every caller of this used to run the parser with `2>/dev/null` and then read
# an empty result as "this coordinate genuinely has no rows" — which is true
# of exactly one of the two ways a python3/awk pipe can print nothing. A
# python3 with no `tomllib`/`tomli` (stock on Ubuntu 22.04's python3.10) raises
# `ModuleNotFoundError` before reading a byte of `examples/fixtures.toml`, and
# that failure's output is ALSO empty — so a caller comparing `$out` to `""`
# cannot tell a dead interpreter from a typo'd platform name, and reported a
# confident, specific, WRONG diagnosis: "no fixture rows for platform
# 'threadx-riscv64'", a name the manifest carries 42 times.
#
# nros_manifest_query <python-script> [args...]
#
# On success: prints the parser's stdout (which may legitimately be empty —
#             that IS "no rows", the case callers are entitled to report as
#             such), returns 0.
# On failure: prints nothing to stdout; prints the parser's own stderr,
#             indented, to THIS caller's stderr, headed by the command that
#             produced it; returns the parser's exit status (never 0, so a
#             caller can distinguish it from a clean empty result).
#
# Follows issue 1249's rule for a status meant to be INSPECTED: the capture is
# `rc=0; out="$(...)" || rc=$?`, never a bare assignment a later `$?` cannot
# reach under `set -e`.
nros_manifest_query() {
    local script="$1"
    shift
    local err_file out rc
    err_file="$(mktemp)" || {
        echo "nros_manifest_query: mktemp failed" >&2
        return 1
    }
    rc=0
    out="$(python3 "$script" "$@" 2>"$err_file")" || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "nros_manifest_query: python3 $script $* failed (exit ${rc}):" >&2
        sed 's/^/  /' "$err_file" >&2
        rm -f "$err_file"
        return "$rc"
    fi
    rm -f "$err_file"
    printf '%s\n' "$out"
}
