# shellcheck shell=bash
# The SDK store's tool dirs a lane puts FIRST on PATH — issue 1638.
#
# `nros sdk-path <tool>` prints where the PINNED tool WOULD live, whether or
# not it was ever provisioned. The Zephyr runner prepended
# `$(nros sdk-path ninja)/bin` unconditionally, and on a host with no store
# ninja that names a directory that does not exist: the prepend was a silent
# no-op, so `ninja` resolved through the rest of PATH — which, inherited from a
# parent shell, named ANOTHER checkout's `third-party/ninja/ninja`, and the
# leaf cached it as `CMAKE_MAKE_PROGRAM`. Nothing said the pin was absent.
#
# nros_sdk_tool_path_prefix <tool>...
#   Prints the `:`-joined `<store>/<tool>/<pin>/bin` dirs that EXIST, in the
#   order given (empty when none do). For each that does not, says so on
#   stderr — naming the provisioning command and the tool PATH will use
#   instead — so a lane's log records which binary it actually ran.
#
# Sourced by `bash` recipes; needs `nros` on PATH.
nros_sdk_tool_path_prefix() {
    local out="" tool dir fallback
    for tool in "$@"; do
        dir="$(nros sdk-path "$tool" 2>/dev/null)/bin"
        if [ -d "$dir" ]; then
            out="${out:+$out:}$dir"
            continue
        fi
        fallback="$(command -v "$tool" 2>/dev/null || true)"
        {
            echo "  note: the pinned $tool is not provisioned (no $dir) — run: nros setup --tool $tool"
            echo "        this lane resolves \`$tool\` by PATH instead: ${fallback:-<none found>}"
        } >&2
    done
    printf '%s' "$out"
}
