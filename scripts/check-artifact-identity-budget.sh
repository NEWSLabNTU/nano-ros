#!/usr/bin/env bash
#
# phase-340 W4 — artifact-identity budget for one workspace at one feature set.
#
# THE PROPERTY
#
# A user with a Rust leaf and a C++ leaf over a shared Rust dependency asks one
# question: is that dependency built ONCE? `examples/workspaces/mixed` is that
# project, and measured 2026-08-06 the answer is EIGHT — `nros-core` compiled
# eight times with eight distinct `-C metadata` identities, zero sharing, at a
# single user-chosen feature set. The split is three ×2 axes, each a different
# cause (phase-340 W4):
#
#   ×2  two corrosion roots — `nano-ros_<h>` (the C++ side) vs
#       `nros_ws_runtime_<h>` (the Rust umbrella). Separate cargo invocations
#       resolving through DIFFERENT workspace manifests, and corrosion keys its
#       target dir on sha1(workspace manifest path), so they cannot even land
#       in the same tree.                                              (R2)
#   ×2  host vs an explicit `--target x86_64-unknown-linux-gnu`, WITHIN one
#       invocation (build scripts / proc macros vs the library).       (R3)
#   ×2  profile. Both fingerprints agree on rustc, features, target, path,
#       rustflags, config, compile_kind and every dep, and differ ONLY in
#       `profile` — the mixed tree carries ten distinct profile hashes at a
#       single `--profile nros-relwithdebinfo`, because
#       `nros_cargo_profile_env()` constructs the corrosion profile from
#       injected `CARGO_PROFILE_*` variables rather than inheriting one.
#
# This gate does not fix any of that. It records the numbers so they cannot
# grow back quietly while W2/W3/W5 are in flight — and so that when one of
# those lands, the drop is a diff in this file rather than a thing someone
# remembers having measured once.
#
# WHAT IT COUNTS
#
# Cargo's `-C metadata` hash — the `lib<crate>-<hash>.rlib` suffix — IS cargo's
# own judgement that two builds are interchangeable, so "same compilation?" is
# answered by construction and not by inspection. Two independent axes, both
# named in W4:
#
#   identities  distinct hashes for one crate  = how many times it was COMPILED
#   copies      dirs holding ONE hash          = how many times the same
#                                                compilation was written out
#
# THE NUMBERS (measured 2026-08-07 on a full native-lane mixed tree)
#
#   nros_core                8 identities   the headline number, pinned exactly
#   any crate               <=9 identities  nros_serdes is the max (8 + one
#                                           more from the nested per-msg-package
#                                           `nros-minsizerel` trees)
#   any single identity     <=5 copies      the five `src/*/nano_ros_cpp_ffi_*/
#                                           target/` trees each write their own
#                                           copy of the SAME hash — R1, the
#                                           per-directory isolation
#
# The named budget pins the crate the phase measured; the two ceilings are the
# class-wide arm, so regrowth in a crate nobody thought to name still fails
# (CLAUDE.md: fix the CLASS, not the reported site).
#
# WHEN A WORK ITEM LANDS, LOWER THE NUMBER. A budget left above the truth is a
# gate that has stopped gating.
#
# WHY `check-fast`
#
# Buildless: it reads FILENAMES under an existing build tree — no cargo, no
# rustc, no workspace resolution, no source submodules. Sub-second. That is the
# fast tier's contract, and it is the tier a developer runs after every task,
# which is exactly when a fresh mixed tree is sitting there to be read.
#
# It is deliberately NOT wired into `build-test-fixtures`: a long-lived
# incremental tree ACCUMULATES rlibs from earlier builds (cargo never collects
# them), so an over-count is possible from history alone, and a gate that can
# fail a BUILD on stale history would be turned off within a week. Failing a
# static check, whose remedy is "delete the tree and rebuild", is survivable.
# The cost is honest and worth stating: on a pristine CI checkout there is no
# tree, so this gate SKIPS there and its live coverage is the local one.
#
# Never silently passes: absent tree => a loud skip naming the build command;
# a tree with artifacts but none for the budgeted crate => a hard failure,
# because that means the gate could not answer the question it exists to ask.
#
# Testing hook: NROS_IDENTITY_BUDGET_TREE points the gate at another tree.

set -uo pipefail
cd "$(dirname "$0")/.."

TREE="${NROS_IDENTITY_BUDGET_TREE:-examples/workspaces/mixed/build-workspace-fixtures}"

# --- the budget -------------------------------------------------------------
# Recorded 2026-08-07. See "THE NUMBERS" above before changing any of these.
BUDGET_CRATE="nros_core"
BUDGET_IDENTITIES=8
CEILING_IDENTITIES=9
CEILING_COPIES=5
# ----------------------------------------------------------------------------

BUILD_HINT="source ./activate.sh && just build-test-fixtures lane=native"

if [ ! -d "$TREE" ]; then
    echo "[SKIP] artifact-identity budget: no build tree at $TREE"
    echo "       This gate reads an existing tree; it never builds one. To give"
    echo "       it something to read:  $BUILD_HINT"
    exit 0
fi

# `*/deps/*` — that is where cargo writes the metadata-suffixed artifacts.
# Anything else with an rlib-shaped name (a staged copy, a vendored blob) is
# not a compilation and must not be counted as one.
#
# `*/out/sizes-probe-target-*/` is PRUNED (phase-340 W6, 2026-08-07). The size
# probe runs a nested cargo build inside `nros-c`'s build script OUT_DIR, one
# per build-script instance, and each rebuilds the dependency graph in its own
# target dir. Those are duplicates of a DIFFERENT kind — the
# duplicate-inside-one-invocation W5 owns, and issue 0464's subject — and
# counting them here conflates two phenomena in one number.
#
# It also made the gate un-actionable. Measured on a tree that had built the
# lane repeatedly: 52 `libnros_core-*.rlib`, of which 40 (76 %) sat under
# sizes-probe dirs, giving 9 identities against a budget of 8 — a FAIL on a
# working tree whose only change was documentation. Excluding them yields
# exactly 8, the recorded budget, which is evidence that the non-probe
# population is what W4 measured when it set the number.
#
# Left in, the count grows with how many times a tree has been built rather
# than with what its source says, and it fails in the fast tier that every task
# runs first — where a red nobody's diff explains is the expensive kind
# (issue 0437).
rlibs="$(find "$TREE" \
    -type d -path '*/out/sizes-probe-target-*' -prune -o \
    -path '*/deps/*' -name 'lib*-*.rlib' -print 2>/dev/null | sort)"

if [ -z "$rlibs" ]; then
    echo "[SKIP] artifact-identity budget: $TREE holds no compiled rlibs"
    echo "       (configured but not built, or built for a non-Rust target)."
    echo "       To give it something to read:  $BUILD_HINT"
    exit 0
fi

# crate<space>hash<space>path, one per artifact.
triples="$(printf '%s\n' "$rlibs" \
    | sed -nE 's|^(.*/lib([A-Za-z0-9_]+)-([0-9a-f]{8,})\.rlib)$|\2 \3 \1|p')"

if [ -z "$triples" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "  $TREE has rlibs under deps/, but none matched lib<crate>-<hash>.rlib." >&2
    echo "  The naming convention this gate reads has changed; fix the gate." >&2
    exit 1
fi

fail=0

# --- the R3 axis, reported (phase-340 W3) -----------------------------------
# Cargo writes an explicitly-targeted unit to `<target-dir>/<triple>/<profile>/`
# and a host unit to `<target-dir>/<profile>/`, so the path says which half of
# the R3 split an artifact came from — and the gate's headline number cannot.
# W4 listed exactly that as a limitation ("it reports a number, not a cause"),
# and W7/item 8 has to lower the budgets per axis rather than in one lump.
#
# This is a REPORT, not a budget. The host half can never reach zero: build
# scripts and proc macros are host units by construction, so an explicitly
# targeted invocation contributes to BOTH columns. What the column does show is
# a whole cargo invocation using the implicit spelling — before phase-340 W3 the
# five `nano_ros_cpp_ffi_*/target/nros-minsizerel/` trees were exactly that.
axis_report() {
    printf '%s\n' "$triples" | awk '
        {
            n = split($3, c, "/")
            # `…/<triple>/<profile>/deps/lib….rlib` → c[n-3] is the triple slot.
            if (c[n-3] ~ /^(x86_64|i686|aarch64|armv7[ar]?|thumbv[0-9]|riscv32|riscv64|arm)[A-Za-z0-9_.]*-/)
                k = "target"
            else
                k = "host"
            ids[$1 SUBSEP $2 SUBSEP k] = 1
            copies[k]++
        }
        END {
            for (i in ids) { split(i, p, SUBSEP); n_ids[p[3]]++ }
            printf "  R3 axis (host vs explicit --target): identities %d/%d, copies %d/%d (host/target)\n",
                n_ids["host"] + 0, n_ids["target"] + 0, copies["host"] + 0, copies["target"] + 0
        }'
}

# --- axis 1: identities per crate (how many times it was COMPILED) ----------
identity_counts="$(printf '%s\n' "$triples" | awk '{print $1, $2}' | sort -u \
    | awk '{print $1}' | uniq -c)"

crate_identities() {
    printf '%s\n' "$identity_counts" | awk -v c="$1" '$2 == c {print $1}'
}

report_crate() {
    # every identity of $1, with the dir it landed in — so a failure says WHICH
    # copies are new, not just that the number moved.
    printf '%s\n' "$triples" | awk -v c="$1" '$1 == c {print "    " $2 "  " $3}'
}

budgeted_n="$(crate_identities "$BUDGET_CRATE")"
if [ -z "$budgeted_n" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "  $TREE holds compiled rlibs, but NONE for $BUDGET_CRATE." >&2
    echo "  The gate cannot answer the question it exists to ask, so it fails" >&2
    echo "  rather than passing on a tree it did not understand. Either the" >&2
    echo "  build is partial (rebuild: $BUILD_HINT) or the crate was renamed," >&2
    echo "  in which case update BUDGET_CRATE in $0." >&2
    exit 1
fi

if [ "$budgeted_n" -gt "$BUDGET_IDENTITIES" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "  $BUDGET_CRATE has $budgeted_n distinct -C metadata identities in $TREE" >&2
    echo "  (budget $BUDGET_IDENTITIES, recorded 2026-08-07 by phase-340 W4)." >&2
    echo "  Each identity is a separate compilation of the same crate:" >&2
    report_crate "$BUDGET_CRATE" >&2
    axis_report >&2
    echo "  A new identity means a new incompatibility axis — workspace root," >&2
    echo "  explicit --target, RUSTFLAGS, or any ONE of opt-level /" >&2
    echo "  debug-assertions / panic / codegen-units / lto / incremental." >&2
    echo "  Target dir is NOT one of them. See phase-340 'The complete" >&2
    echo "  incompatibility set'. If a long-lived tree accumulated these from" >&2
    echo "  earlier builds, delete $TREE and rebuild before believing the count." >&2
    fail=1
fi

over_ceiling="$(printf '%s\n' "$identity_counts" \
    | awk -v k="$CEILING_IDENTITIES" '$1 > k {print $2, $1}')"
if [ -n "$over_ceiling" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "  crates over the tree-wide ceiling of $CEILING_IDENTITIES identities in $TREE:" >&2
    while read -r crate n; do
        echo "  $crate: $n" >&2
        report_crate "$crate" >&2
    done <<< "$over_ceiling"
    fail=1
fi

# --- axis 2: copies of ONE identity (R1 — per-directory isolation) ----------
over_copies="$(printf '%s\n' "$triples" | awk '{print $1, $2}' | sort | uniq -c \
    | awk -v k="$CEILING_COPIES" '$1 > k {print $2, $3, $1}')"
if [ -n "$over_copies" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "  identities written into more than $CEILING_COPIES target dirs in $TREE:" >&2
    while read -r crate hash n; do
        echo "  $crate $hash: $n copies" >&2
        printf '%s\n' "$triples" | awk -v h="$hash" '$2 == h {print "    " $3}' >&2
    done <<< "$over_copies"
    echo "  Cargo already judged these interchangeable; they are the same" >&2
    echo "  compilation repeated per directory (phase-340 R1)." >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    exit 1
fi

max_ids="$(printf '%s\n' "$identity_counts" | awk '{print $1}' | sort -rn | head -1)"
max_copies="$(printf '%s\n' "$triples" | awk '{print $1, $2}' | sort | uniq -c \
    | awk '{print $1}' | sort -rn | head -1)"
echo "artifact-identity budget OK ($TREE): $BUDGET_CRATE $budgeted_n/$BUDGET_IDENTITIES identities;" \
     "worst crate $max_ids/$CEILING_IDENTITIES; worst identity $max_copies/$CEILING_COPIES copies."
axis_report
