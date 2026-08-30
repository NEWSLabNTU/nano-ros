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

# phase-366 W6 — the two lang items an image may hold exactly one of. `panic`
# joins `alloc` because they are the same defect (issue 0618): a singleton of the
# FINAL ARTIFACT, decided in a library. `rust_begin_unwind` is what
# `#[panic_handler]` emits, verified against
# `examples/workspaces/mixed/build-workspace-fixtures-freertos/libnros_ws_runtime.a`
# rather than assumed.
LANG_ITEMS=("__rust_alloc:the global allocator" "rust_begin_unwind:the panic handler")

# issue 0642 — `--list` prints the link lines this scan WOULD check, and exits.
# The prune below is only defensible against a measurement, and this is the half
# of that measurement that lives in the script: diff it against an unpruned
# `find` and every difference should be vendored `out/` or `nros-metadata`.
list_only=0
if [ "${1:-}" = "--list" ]; then
    list_only=1
    shift
fi

roots=("$@")
if [ ${#roots[@]} -eq 0 ]; then
    roots=(examples packages build)
fi

command -v nm >/dev/null 2>&1 || {
    echo "check-archive-lang-items: SKIP — no \`nm\` on this host" >&2
    exit 0
}

# Does this archive DEFINE the given lang item (`T`), rather than reference it
# (`U`)? Memoised: the same `.a` appears on many link lines.
declare -A defines_cache=()
# defines_symbol <archive> <symbol>
defines_symbol() {
    local a="$1"
    # Key on archive AND symbol: the same `.a` is asked about once per lang item.
    local key="$a::$2"
    if [ -n "${defines_cache[$key]:-}" ]; then
        [ "${defines_cache[$key]}" = "yes" ]
        return
    fi
    local hits
    # NOT `| grep -q`: with `set -o pipefail`, grep's early exit gives `nm`
    # SIGPIPE and the pipeline reports FAILURE on a match — which inverted an
    # earlier revision of this gate silently. It passed on a pair of archives
    # measured by hand to both define the symbol. Capture and test instead.
    hits="$(nm -g "$a" 2>/dev/null | grep -E "^[0-9a-f]+ T .*${2}\$" || true)"
    if [ -n "$hits" ]; then
        defines_cache["$key"]="yes"; return 0
    fi
    defines_cache["$key"]="no"; return 1
}

scan_link_lines() {
    for r in "${roots[@]}"; do
        [ -d "$r" ] || continue
        find "$r" -xdev \
            \( -type d \( -name .git -o -name deps -o -name incremental \
                        -o -name .fingerprint -o -name out -o -name nros-metadata \) \
               -prune \) -o \
            -type f -name 'link.txt' -path '*CMakeFiles*' -print 2>/dev/null
    done
}

if [ "$list_only" -eq 1 ]; then
    scan_link_lines
    exit 0
fi

links=0
rc=0
while IFS= read -r lt; do
    links=$((links + 1))
    dir="$(dirname "$lt")"
    declare -A owners_by_item=()
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
        for item in "${LANG_ITEMS[@]}"; do
            sym="${item%%:*}"
            if defines_symbol "$cand" "$sym"; then
                owners_by_item["$sym"]+="$tok "
            fi
        done
    done < <(tr " " "\n" < "$lt")

    for item in "${LANG_ITEMS[@]}"; do
        sym="${item%%:*}"
        what="${item#*:}"
        # shellcheck disable=SC2206
        owners=(${owners_by_item[$sym]:-})
        if [ "${#owners[@]}" -gt 1 ]; then
            rc=1
            echo "check-archive-lang-items: $lt links ${#owners[@]} archives that each define $what:" >&2
            for o in "${owners[@]}"; do echo "    $o" >&2; done
        fi
    done
done < <(
    # issue 0642 — PRUNE. This walk used to descend into everything under
    # `examples packages build` and cost ~22 MINUTES of wall clock against ~15
    # seconds of CPU: pure I/O over millions of object files, paid by every
    # `build-test-fixtures`. `-path '*CMakeFiles*'` cannot help, because find has
    # to reach a path before it can test it.
    #
    # Measured on this tree (260 link.txt total, unpruned walk as the truth set):
    #
    #   unpruned          260 files   ~22 min
    #   pruned (below)     89 files   1.1 s
    #   difference        171 files   = 138 vendored + 33 probe residue + 0 real
    #
    # What each prune drops, and why it cannot hide a real image:
    #
    #   deps, incremental, .fingerprint  cargo internals. No cmake target lives
    #                                    in them; they hold most of the inodes.
    #   out                              a cargo build script's OUT_DIR. The
    #                                    link lines under it are VENDORED cmake
    #                                    builds — `cyclonedds-sys-*/out/build/`
    #                                    linking CycloneDDS's own `ddsc`,
    #                                    `ddsrt-internal`, `idl`, `ddsperf`.
    #                                    Third-party internals are not nano-ros
    #                                    images and this gate has no claim on
    #                                    them.
    #   nros-metadata                    metadata-PROBE residue, which is what
    #                                    made this gate fail on 16-day-old
    #                                    gitignored output in the first place.
    #                                    A probe's link line is not an image's.
    #   .git                             never build output.
    #
    # The 0 in that table is the load-bearing number: every excluded path is in
    # one of those two categories, so the fast scan sees every link line the slow
    # one did that this gate is about. Re-derive it with
    # `comm -23 <(unpruned) <(pruned)` if a prune is ever added here.
    scan_link_lines
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

# issue 0642 — a gate that matches nothing must SAY so.
#
# The scan is pruned (see above), and the failure mode of a prune is silence:
# exclude one directory too many and this prints "OK (0 link line(s))" while
# checking nothing. That is the issue-0196 shape — a gate whose coverage quietly
# became narrower than the rule it enforces.
#
# Not fatal, because 0 is legitimate on a tree where no cmake image has been
# built yet (a fresh clone running `just check archive-lang-items` by hand). It
# is never legitimate after a fixture build, and the line says which case the
# reader is in.
if [ "$links" -eq 0 ]; then
    echo "check-archive-lang-items: WARNING — no link lines matched." >&2
    echo "  Nothing was checked. Expected after a fresh clone (no cmake image built" >&2
    echo "  yet); NOT expected after \`just build-test-fixtures\`, where it means the" >&2
    echo "  prune list in this script has grown too broad. Re-derive the exclusions:" >&2
    echo "    comm -23 <(find examples packages build -name link.txt -path '*CMakeFiles*' | sort) \\" >&2
    echo "             <(bash scripts/check-archive-lang-items.sh --list | sort)" >&2
    exit 0
fi

echo "check-archive-lang-items: OK ($links link line(s), no image links two archives defining one lang item)"
