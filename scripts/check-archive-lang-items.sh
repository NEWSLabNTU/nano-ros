#!/usr/bin/env bash
# At most ONE Rust archive per LINK LINE may define the global allocator.
#
# ## What this is NOT
#
# It is not the fix for issue 0616, and it could not have caught it. That
# failure was two `-C metadata` identities of `nros-platform` inside ONE
# compile's closure — two cargo workspace roots sharing a `--target-dir`, whose
# fingerprints differed only in the `path` spelling — and it fired at rustc
# time, before any archive existed. `84d25f1f8` fixed it by deriving the target
# dir from `cargo locate-project --workspace`; CLAUDE.md carries the rule.
#
# ## What it IS
#
# A guard for the sibling hazard that investigation measured and left standing.
# `#[global_allocator]` is unique per LINKED ARTIFACT, but nano-ros declares it
# in `nros-platform`, a mid-graph library, gated on a feature. `packages/` has
# FOUR `crate-type = ["staticlib"]` roots — `nros-c`, `nros-cpp`,
# `nros-rmw-zenoh-staticlib`, `nros-rmw-xrce-cffi-staticlib` — and each BAKES
# that one definition into its own `.a` when `global-allocator` is on. Measured
# with `nm -g`: `libnros_c.a` and `libnros_rmw_zenoh_staticlib.a` each export
# `___rustc___rust_alloc` as a global `T`.
#
# Nothing links two of them TODAY, because the only Zephyr target ever built is
# `native_sim`, where `zephyr/CMakeLists.txt` appends `,std` and the host's std
# supplies the lang items — so `nros-platform`'s `not(feature = "std")` gate is
# off. Point the same wiring at a real board (`platform-zephyr-baremetal`
# enables `nros-platform/global-allocator`, and so does `nros-c/platform-zephyr`
# via the umbrella) and both archives carry it. That is a first-real-board
# failure with a message three crates from its cause, which is the kind worth
# paying a gate for before it happens rather than after.
#
# `check-feature-contract` clause (e) cannot make this check: it counts
# `#[global_allocator]` DEFINITIONS IN SOURCE, where there is exactly one and
# always will be. The count that matters is per shipped artifact.
#
# ## How
#
# Reads cmake `link.txt` files — the real per-image archive set. Grouping by
# DIRECTORY was tried first and is wrong: a cargo target dir holds every
# staticlib the workspace can produce (an earlier revision fired on a `deps/`
# dir with nine, and on any dir holding both `libnros_c.a` and `libnros_cpp.a`),
# and none of those sets is an image. Only a link line says what one image gets.
#
set -uo pipefail
cd "$(dirname "$0")/.."

# rustc mangles the shim into the `__rustc` namespace under v0; both spellings
# have been seen depending on toolchain, so match either.
SYM_RE='(^|[^A-Za-z0-9_])__rust_alloc$'

roots=("$@")
if [ ${#roots[@]} -eq 0 ]; then
    roots=(examples packages build)
fi

command -v nm >/dev/null 2>&1 || {
    echo "check-archive-lang-items: SKIP — no \`nm\` on this host" >&2
    exit 0
}

# Does this archive DEFINE the allocator shim (`T`), rather than reference it
# (`U`)? Memoised: the same `.a` appears on many link lines.
declare -A defines_cache=()
defines_allocator() {
    local a="$1"
    if [ -n "${defines_cache[$a]:-}" ]; then
        [ "${defines_cache[$a]}" = "yes" ]
        return
    fi
    local hits
    # NOT `| grep -q`: with `set -o pipefail`, grep's early exit gives `nm`
    # SIGPIPE and the pipeline reports FAILURE on a match — which inverted an
    # earlier revision of this gate silently. It passed on a pair of archives
    # measured by hand to both define the symbol. Capture and test instead.
    hits="$(nm -g "$a" 2>/dev/null | grep -E "^[0-9a-f]+ T .*__rust_alloc$" || true)"
    if [ -n "$hits" ]; then
        defines_cache["$a"]="yes"; return 0
    fi
    defines_cache["$a"]="no"; return 1
}

links=0
rc=0
while IFS= read -r lt; do
    links=$((links + 1))
    dir="$(dirname "$lt")"
    owners=()
    # cmake writes the link line space-separated; archives may be relative to
    # the build dir the link runs in, which is the ancestor holding CMakeFiles.
    base="${dir%%/CMakeFiles/*}"
    while IFS= read -r tok; do
        case "$tok" in
            *.a) ;;
            *) continue ;;
        esac
        cand="$tok"
        [ -f "$cand" ] || cand="$base/$tok"
        [ -f "$cand" ] || continue
        if defines_allocator "$cand"; then
            owners+=("$tok")
        fi
    done < <(tr " " "\n" < "$lt")

    if [ "${#owners[@]}" -gt 1 ]; then
        rc=1
        echo "check-archive-lang-items: $lt links ${#owners[@]} archives that each define the global allocator:" >&2
        for o in "${owners[@]}"; do echo "    $o" >&2; done
    fi
done < <(
    for r in "${roots[@]}"; do
        [ -d "$r" ] && find "$r" -name 'link.txt' -path '*CMakeFiles*' -type f 2>/dev/null
    done
)

if [ "$rc" -ne 0 ]; then
    {
        echo
        echo "  Each is a link root that baked \`nros-platform\`'s \`#[global_allocator]\`"
        echo "  into itself. An image linking two of them has two allocators, which rustc"
        echo "  reports as \"the #[global_allocator] in nros_platform conflicts with global"
        echo "  allocator in: nros_platform\" — one crate named on both sides, and nothing"
        echo "  wrong in the dependency graph, because the duplication is in the LINK."
        echo
        echo "  Fix at the link, not in the source: one Rust staticlib per image"
        echo "  (phase-241 W11). A backend reaches the umbrella as an rlib —"
        echo "  \`nros-c\`'s \`rmw-zenoh = [\"rmw-cffi\", \"dep:nros-rmw-zenoh\"]\` — so a"
        echo "  standalone backend archive beside it is redundant AND a second set of"
        echo "  lang items. See issue 0616, and issue 0436 for the same class on px4."
    } >&2
    exit 1
fi

echo "check-archive-lang-items: OK ($links link line(s), no image links two allocator-defining archives)"
