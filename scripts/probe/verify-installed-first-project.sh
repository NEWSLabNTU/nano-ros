# shellcheck shell=bash
# PROBE-OWNED verification for the `installed` track (phase-447 A3, RFC-0099
# D1). Appended after the book-extracted steps by run-bootstrap-probe.sh; runs
# in the same shell, so cwd is the workspace multi-node-linux.md scaffolded and
# built.
#
# The book steps have already done the thing under test — install a release,
# provision, scaffold, configure, build — and check-installed-sdk-root.sh ran
# right after the install to prove there is no checkout and the SDK root is the
# release's own. What is left is the Run step, whose documented command never
# exits, plus one re-check that nothing since the install planted a nano-ros
# root the build could have resolved instead.

echo '=== probe verify: the installed path, with no checkout ==='
probe_fail() {
    echo "PROBE FAIL: $*" >&2
    exit 1
}

nros_store="$HOME/.nros"
marker_hits="$(find / -xdev \( -path /proc -o -path "$nros_store" \) -prune -o \
    -path '*/packages/core/nros-core/Cargo.toml' -print 2>/dev/null || true)"
[ -z "$marker_hits" ] \
    || probe_fail "a nano-ros root appeared outside the store during setup/build: $marker_hits"

# --- the build used what `nros setup` provisioned (issue 1304) -----------------
# installation.md says `nros setup linux --rmw cyclonedds` provisions "Cyclone
# DDS runtime + idlc", so the build must link THAT, not a Cyclone compiled from
# source behind the user's back. Asserted here because the source fallback also
# produces a working binary: without this check, a configure that stopped
# finding the provisioned dist would still pass the Run step below, and the
# prebuilt RFC-0099 D4 prefers would silently stop being used.
cache=./build/posix-cyclonedds-linux/cmake/CMakeCache.txt
[ -f "$cache" ] || probe_fail "no $PWD/${cache#./} — the Build step never configured (`nros build` drives cmake under build/<coord>/cmake)"
cyclone_dir="$(sed -n 's/^CycloneDDS_DIR:[A-Z]*=//p' "$cache")"
# The prefix is ASKED for, never matched with a version wildcard (issue 0625,
# `check-sdk-store-not-enumerated`), and asked from the SDK root so it reads the
# index that root ships — the same question nros-rmw-provision.cmake asks.
rc=0
want="$(cd "$(nros dev sdk root)" && nros dev sdk path cyclonedds --require)" || rc=$?
[ "$rc" -eq 0 ] && [ -n "$want" ] \
    || probe_fail "\`nros dev sdk path cyclonedds --require\` exited $rc — \`nros setup --rmw cyclonedds\` provisioned no Cyclone where the build looks"
case "$cyclone_dir" in
    "$want"/*)
        echo "probe: CycloneDDS came from the prebuilt nros setup provisioned ($cyclone_dir)" ;;
    *)
        probe_fail "the build did not use the Cyclone \`nros setup --rmw cyclonedds\` provisioned: CycloneDDS_DIR='$cyclone_dir', want one under $want (issue 1304, RFC-0099 D4)" ;;
esac

# --- the Run step, non-interactively -------------------------------------------
# The Run step is the chapter's own: its `nros run ... --expect` block exits
# non-zero unless the talker published, so the book step already asserted it
# (phase-532: first-project.md's Linux variant runs one talker through
# `nros run` instead of a hand-run entry binary).
# phase-522 W5 — the installed `nros` states this board's support status, read
# from the registry its own SDK root ships (RFC-0114). No tier number: a status
# word from RFC-0114 D10's table.
if [ -n "${PROBE_BOARD:-}" ]; then
    if ! show="$(nros board show "$PROBE_BOARD")"; then
        echo "PROBE FAIL: \`nros board show $PROBE_BOARD\` failed on the installed toolchain" >&2
        exit 1
    fi
    status_line="$(printf '%s\n' "$show" | grep '^  status ' || true)"
    case "$status_line" in
        *Verified*|*Boots*|*Builds*|*"Not supported"*) echo "probe: board $PROBE_BOARD:$status_line" ;;
        *) echo "PROBE FAIL: \`nros board show $PROBE_BOARD\` printed no status word: $show" >&2
           exit 1 ;;
    esac
fi

echo "PROBE PASS: installed release -> scaffold -> build -> run, with no nano-ros checkout on the host"
