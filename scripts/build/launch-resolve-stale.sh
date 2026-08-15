# Is the built `nros-launch-resolve` older than its own SOURCES? — issue 0596.
#
# Two sites used to ask a different question — "is the resolver binary older
# than the CLI binary?" — and that question has no clearable answer:
# `just setup-launch-resolve` delegates to cargo, cargo correctly does nothing
# when the resolver's sources have not changed, so the binary is never relinked
# and its mtime never moves. After any `setup-cli` the warning fired forever,
# with a remedy that provably could not silence it.
#
# It was also the wrong question in the other direction. The hazard (issue
# 0363 C) is that the CLI and the resolver must agree on an ARGUMENT LIST, and
# that can only drift when one of them is built from stale SOURCES. Touching the
# resolver binary would have silenced the mtime check while proving nothing, and
# a genuine skew whose binary happened to be newer went undetected.
#
# So ask about sources, which is both the real invariant and clearable by the
# remedy. The walk mirrors `setup-launch-resolve`'s own probe deliberately: the
# resolver tree lives INSIDE the play_launch submodule, so `git ls-files` must
# run with `-C` there (from the superproject the index holds only the gitlink and
# would match nothing, making every pin bump look current — the museum-binary
# failure that probe exists to catch). Scoped to layer 2
# (`src/ros-launch-resolve`, regular files); play_launch's layer-3 submodules are
# deliberately uninitialised and are neither walked nor required.

# nros_launch_resolve_stale <repo-root>
#
# Exit 0 (true) when the binary is missing or older than a tracked source file.
# Exit 1 (false) when it is current — or when the submodule is not initialised,
# which `setup-launch-resolve` reports on its own terms and this must not
# duplicate.
nros_launch_resolve_stale() {
    local root="${1:-.}"
    local crate="$root/packages/cli/nros-launch-resolve"
    local pl="$root/packages/cli/third-party/play_launch"
    # profile-literal-ok: host tool: the launch resolver's own binary
    local bin="${CARGO_TARGET_DIR:-$crate/target}/release/nros-launch-resolve"

    [ -x "$bin" ] || return 0
    [ -d "$pl/src/ros-launch-resolve" ] || return 1

    local f
    while IFS= read -r f; do
        [ -e "$f" ] || continue
        if [ "$f" -nt "$bin" ]; then
            return 0
        fi
    done < <( { git -C "$root" ls-files "$crate" 2>/dev/null | grep -E '\.rs$|Cargo\.toml$' \
                    | sed "s|^|$root/|"
                git -C "$pl" ls-files "src/ros-launch-resolve" 2>/dev/null \
                    | grep -E '\.rs$|Cargo\.toml$' | sed "s|^|$pl/|" ; } )
    return 1
}
