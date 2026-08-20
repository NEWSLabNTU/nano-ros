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

# phase-363 — CONTENT, not mtime. Asking about sources fixed the "remedy cannot
# clear it" half of issue 0596; the comparison was still `source -nt binary`,
# which `git rebase`, `git stash pop` and `git checkout` all falsify by
# rewriting tracked files with IDENTICAL content. `source_stamp.rs` records the
# same reasoning for the CLI: "hash what the sources ARE instead of when they
# were written. A rebase becomes silent; an actual edit is still caught."
#
# `Cargo.lock` joins `.rs`/`Cargo.toml` because it pins what the binary was
# built FROM — a lock move is a different build. The other 160 tracked files in
# the resolver tree stay out on evidence rather than by omission: 67 are `.py`
# belonging to layer 2's CPython runtime, and nothing embeds them
# (`include_str!`/`include_bytes!` find nothing, and the tree has no
# `build.rs`), so they cannot change the Rust binary. The rest are
# README/LICENSE/.gitignore. Watching them would force a rebuild on every
# docs edit, which is the cost `source_stamp.rs` avoids by matching precisely.
#
# nros_launch_resolve_stale <repo-root>
#
# Exit 0 (true) when the binary is missing, has no recorded stamp, or its stamp
# disagrees with the sources on disk. Exit 1 (false) when it is current — or
# when the submodule is not initialised, which `setup-launch-resolve` reports on
# its own terms and this must not duplicate.
nros_launch_resolve_stale() {
    local root="${1:-.}"
    local crate="$root/packages/cli/nros-launch-resolve"
    local pl="$root/packages/cli/third-party/play_launch"
    # profile-literal-ok: host tool: the launch resolver's own binary
    local bin="${CARGO_TARGET_DIR:-$crate/target}/release/nros-launch-resolve"

    [ -x "$bin" ] || return 0
    [ -d "$pl/src/ros-launch-resolve" ] || return 1

    local recorded
    recorded="$(cat "$bin.nros-source-stamp" 2>/dev/null || true)"
    [ -n "$recorded" ] || return 0

    [ "$(nros_launch_resolve_stamp "$root")" = "$recorded" ] && return 1
    return 0
}

# The content stamp itself, so the writer (`setup-launch-resolve`) and the
# reader above cannot compute it differently — the defect issue 0363 names as
# "the predicate lived in three places, two of them real implementations that
# could disagree".
#
# Sorted so the digest is content-determined rather than filesystem-determined,
# and missing files are skipped so a partially-checked-out tree degrades to a
# different stamp rather than an error.
nros_launch_resolve_stamp() {
    local root="${1:-.}"
    local crate="$root/packages/cli/nros-launch-resolve"
    local pl="$root/packages/cli/third-party/play_launch"
    {
    {
        git -C "$root" ls-files "$crate" 2>/dev/null \
            | grep -E '\.rs$|Cargo\.(toml|lock)$' | sed "s|^|$root/|"
        git -C "$pl" ls-files "src/ros-launch-resolve" 2>/dev/null \
            | grep -E '\.rs$|Cargo\.(toml|lock)$' | sed "s|^|$pl/|"
    } | sort | while IFS= read -r f; do
        # REPO-RELATIVE path in the digest. `sha256sum` prints "<hash>  <path>",
        # so hashing that line verbatim makes the stamp depend on how the caller
        # spelled the root — `setup-launch-resolve` passes an absolute
        # `justfile_directory()` while the precondition script passes `.`, and
        # the two produced different digests for an identical tree. The check
        # then reported stale immediately after its own remedy, which is the
        # exact symptom issue 0596 is about.
        [ -f "$f" ] && printf '%s  %s\n' "$(sha256sum "$f" | awk '{print $1}')" "${f#"$root"/}"
    done
    # The play_launch PIN, not just layer-2's content — issue 0561's fix,
    # applied to the second of the two binaries that stamp it.
    #
    # `build.rs` bakes the submodule commit into BOTH `nros` and
    # `nros-launch-resolve` as `NROS_PLAY_LAUNCH_SHA`, and `verify_resolver_pin`
    # compares those two COMMITS. This probe compared layer-2 CONTENT. The two
    # identities agree only while every pin move touches `src/ros-launch-resolve`
    # — and 420904826055..65a7591e5165 touched `tests/**` and nothing else. So
    # the content stamp matched, `setup-launch-resolve` exited 0 without
    # rebuilding, the binary kept the old commit, and `nros sync` refused with a
    # remedy that provably could not clear it. `build-test-fixtures` was
    # unrunnable at that pin, and re-running the printed command forever was the
    # only thing it suggested.
    #
    # 0561 records this same failure for `nros` itself ("moving the pin left the
    # stamp unchanged, `setup-cli` skipped the rebuild while reporting success,
    # and no sanctioned command could clear the resulting mismatch") and fixed it
    # in `source_stamp.rs`. It fixed ONE of the two stampers.
    #
    # Gate on the `.git` FILE, per issue 0419: an uninitialised submodule is an
    # empty directory that EXISTS, and `git -C <empty dir> rev-parse HEAD` walks
    # UP to the superproject — which would move this component with every
    # nano-ros commit and re-stale the resolver constantly.
    if [ -e "$pl/.git" ]; then
        printf '%s  play_launch@pin\n' \
            "$(git -C "$pl" rev-parse HEAD 2>/dev/null || echo unknown)"
    fi
    } | sha256sum | awk '{print $1}'
}
