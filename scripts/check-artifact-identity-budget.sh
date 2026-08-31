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
#   nros_core                4 identities   the headline number, pinned exactly
#   any crate              <=12 identities  `nros` is the max
#   any single identity     <=5 copies      the five `src/*/nano_ros_cpp_ffi_*/
#                                           target/` trees each write their own
#                                           copy of the SAME hash — R1, the
#                                           per-directory isolation
#
# **The 2026-08-07 numbers were produced by a broken counter and are NOT
# comparable to these** (found 2026-08-10 — see "axis 1" below). `uniq -c` on
# locale-collated input reported `nros` twice, as 7 and 5, so the tree-wide
# ceiling of 9 was compared against two halves of a crate that actually had 12.
# It never measured what it claimed. The old note here read "nros_serdes is the
# max" at 8-9; `nros_serdes` measures 5, and it is not the max.
#
# So `12` is not a raised ceiling — it is the FIRST honest reading of this axis,
# and it decomposes exactly:
#
#     2 workspace roots  x  2 R3 halves  x  3 feature identities  =  12
#
# `nano-ros_23c15` and `nros_ws_runtime_16b35` are the roots (the "22/22 leaves
# are workspace roots" fact Wave 1 measured); host `debug/deps` vs explicit
# `x86_64-unknown-linux-gnu/debug/deps` is the R3 split phase-340 W3 made
# universal. Nothing here is unexplained, which is the precondition item 8
# demanded before any number moved.
#
# `nros_core` drops 8 -> 4 in the same edit: it reads 4 and has read 4 across
# every session, it is contiguous under any collation (its four hashes start
# 0/4/6/9), and item 8 named it as separately well understood.
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

# issue 0726 — the selftest below branches on `grep -q` over captured output. A
# grep that failed to START would read as "the gate printed no verdict", which
# is a finding, so use `nros_grep_q` (exit 2 on a tool failure) and a
# HERESTRING, not a pipe: a pipeline segment is a subshell and its `exit` would
# end only that subshell.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

# issue 0661 — `--self-test`: the two era verdicts must match their own note.
#
# The failure this guards is not a wrong COUNT but wrong ADVICE. When the
# started_at window says nothing about the budgeted crate, the gate widens to
# the whole tree and says so — and used to then announce "accumulation is ruled
# out — do NOT delete the tree", because the flag `era_verdict()` reads was
# initialised after that widening. A reader who believes it goes looking for a
# real regression in an accumulated tree, which is exactly how issue 0661 spent
# its length blaming two ordinary host build-dependency artifacts.
#
# Both directions are checked: a filtered reading must still say "ruled out"
# (else the fix has made every reading unfiltered and the gate stops
# distinguishing).
if [ "${1:-}" = "--self-test" ]; then
    _st_tmp="$(mktemp -d)"
    trap 'rm -rf "$_st_tmp"' EXIT
    _st_deps="$_st_tmp/tree/cargo/root_a/x86_64-unknown-linux-gnu/profile/deps"
    mkdir -p "$_st_deps"
    # Five identities of the budget crate: over any sane budget, so the gate
    # fails and prints a verdict.
    for h in 1111111111111111 2222222222222222 3333333333333333 \
             4444444444444444 5555555555555555; do
        : > "$_st_deps/libnros_core-$h.rlib"
    done
    : > "$_st_deps/libother-9999999999999999.rlib"
    _st_fails=0

    # (a) window EXCLUDES the crate -> unfiltered -> "rebuild and re-read".
    _st_stamp_a="$_st_tmp/stamp_a"
    touch -d '2000-01-01 00:00:00' "$_st_deps"/libnros_core-*.rlib
    printf 'started_at=2030-01-01T00:00:00Z\n' > "$_st_stamp_a"
    # `other` inside the window keeps `rlibs` non-empty, which is what makes
    # this the EARLY widening rather than the stampless path.
    touch -d '2035-01-01 00:00:00' "$_st_deps/libother-9999999999999999.rlib"
    _st_out="$(NROS_IDENTITY_BUDGET_TREE="$_st_tmp/tree" NROS_FIXTURE_STAMP="$_st_stamp_a" \
        bash "$0" 2>&1 || true)"
    if nros_grep_q 'accumulation is ruled out' <<<"$_st_out"; then
        echo "  FAIL (a): an UNFILTERED reading claimed accumulation was ruled out" >&2
        _st_fails=$((_st_fails + 1))
    elif nros_grep_q 'UNFILTERED' <<<"$_st_out"; then
        echo "  ok   (a) a widened reading says so and asks for a rebuild"
    else
        echo "  FAIL (a): no era verdict printed at all" >&2
        _st_fails=$((_st_fails + 1))
    fi

    # (b) window INCLUDES the crate -> filtered -> "do not delete the tree".
    # Dates are far apart on purpose: a stamp EQUAL to an mtime sits on the
    # comparison's boundary, and which side it lands on is not what this test
    # is about.
    _st_stamp_b="$_st_tmp/stamp_b"
    touch -d '2020-01-01 00:00:00' "$_st_deps"/libnros_core-*.rlib
    printf 'started_at=2010-01-01T00:00:00Z\n' > "$_st_stamp_b"
    _st_out="$(NROS_IDENTITY_BUDGET_TREE="$_st_tmp/tree" NROS_FIXTURE_STAMP="$_st_stamp_b" \
        bash "$0" 2>&1 || true)"
    if nros_grep_q 'accumulation is ruled out' <<<"$_st_out"; then
        echo "  ok   (b) a filtered reading still keeps the evidence"
    else
        echo "  FAIL (b): a filtered reading lost its 'do not delete' advice" >&2
        _st_fails=$((_st_fails + 1))
    fi

    # (c) NO stamp, but the SIDECAR survives — a build that started and died.
    #
    # This is the wedge: `nros_fixtures_stamp_clear()` removes the stamp and
    # writes `<stamp>.started`; only SUCCESS folds it in. A failed build leaves
    # exactly this state, and the gate used to count the whole tree — an
    # unattributable number — while `build-test-fixtures` lists `check::fast` as
    # a dependency, so the gate then blocked the rebuild that would fix it.
    #
    # Both halves are asserted: the bound is USED (not the whole-tree path), and
    # the reading SAYS the build was partial. A filtered count under wording that
    # implies a completed build is issue 0661's defect wearing a new hat.
    _st_stamp_c="$_st_tmp/stamp_c"
    rm -f "$_st_stamp_c"
    touch -d '2020-01-01 00:00:00' "$_st_deps"/libnros_core-*.rlib
    printf '2010-01-01T00:00:00Z\n' > "$_st_stamp_c.started"
    _st_out="$(NROS_IDENTITY_BUDGET_TREE="$_st_tmp/tree" NROS_FIXTURE_STAMP="$_st_stamp_c" \
        bash "$0" 2>&1 || true)"
    if nros_grep_q 'NO started_at' <<<"$_st_out"; then
        echo "  FAIL (c): the sidecar bound was ignored and the whole tree counted" >&2
        _st_fails=$((_st_fails + 1))
    elif ! nros_grep_q 'PARTIAL build' <<<"$_st_out"; then
        echo "  FAIL (c): filtered on the sidecar without saying the build was partial" >&2
        _st_fails=$((_st_fails + 1))
    else
        echo "  ok   (c) a sidecar bound is used, and says the build did not finish"
    fi

    if [ "$_st_fails" -ne 0 ]; then
        echo "check-artifact-identity-budget --self-test: $_st_fails case(s) FAILED" >&2
        exit 1
    fi
    echo "check-artifact-identity-budget --self-test: 3 case(s) OK"
    exit 0
fi

# issue 0901 — RESOLVE the tree, do not hardcode one path.
#
# This read `examples/workspaces/mixed/build-workspace-fixtures` literally. That
# directory has no producer any more: the workspace build gained PER-PLATFORM
# suffixes (`-freertos`, `-threadx`, …) and the unsuffixed name was left behind.
# So on a fresh checkout the gate SKIPPED — the tree cannot exist — while on a
# long-lived machine it read a directory nothing had written since the rename
# and reported pure accumulation as a budget breach. Measured here: 8 identities
# for `nros` against a ceiling of 5, entirely history, and a full
# `build-test-fixtures lane=native` (2 616 s) did not recreate the path, which is
# what the SKIP message told the user to run.
#
# Both failure modes come from naming a path instead of finding one. Take the
# NEWEST tree that actually has cargo artifacts, so the gate reads what the last
# build produced whatever the layout is called this month.
if [ -n "${NROS_IDENTITY_BUDGET_TREE:-}" ]; then
    TREE="$NROS_IDENTITY_BUDGET_TREE"
else
    TREE=""
    for _cand in $(ls -1dt examples/workspaces/*/build-workspace-fixtures* 2>/dev/null); do
        # A tree is usable when it holds cargo output; a bare cmake dir is not
        # what this gate measures.
        if [ -n "$(find "$_cand" -name '*.rlib' -print -quit 2>/dev/null)" ]; then
            TREE="$_cand"
            break
        fi
    done
    # Nothing found: keep the historical name so the SKIP message still points
    # somewhere recognisable rather than at an empty string.
    TREE="${TREE:-examples/workspaces/mixed/build-workspace-fixtures}"
fi

# --- the budget -------------------------------------------------------------
# Recorded 2026-08-07. See "THE NUMBERS" above before changing any of these.
BUDGET_CRATE="nros_core"
BUDGET_IDENTITIES=4
# phase-340 item 8, 2026-08-10 — lowered 12 -> 6, then 6 -> 5 on a REBUILT tree.
#
# The worst crate is `nros_serdes`. The 6 was read on a tree first built before
# W3's cmake half landed (2026-08-08); a fresh `workspace-fixtures-build.sh
# linux mixed` reads **5**, and the missing one was dead output, not a
# compilation the current build can produce.
#
# The 6 was decomposed here as "THREE identity-pairs x TWO `--target`
# spellings", with the prediction that it would fall to 3 when W3's cargo-LEAF
# half landed. **Both halves of that are wrong, and the fingerprints say so**
# (measured 2026-08-10, `<root>/<profile>/.fingerprint/nros-serdes-*/lib-*.json`):
#
#   pair                       what actually differs between its two members
#   -------------------------  --------------------------------------------
#   cargo/<cpp-root>           features [] vs ["alloc","std"]; profile
#   cargo/<ws-runtime-root>      (build-override vs product); rustflags [] vs
#                                ["-C","symbol-mangling-version=v0"];
#                                compile_kind host vs triple  -> FOUR fields
#   src/*/nano_ros_cpp_ffi_*   compile_kind ONLY  -> a true spelling pair
#
# The first two are not `--target` pairs at all. They are the PROC-MACRO graph:
# `nros-macros` -> `nros-orchestration-ir` -> `nros-rmw` -> `nros-core` ->
# `nros-serdes`, which cargo compiles for the host at the build-override profile
# with no features and no rustflags, by construction, inside the SAME explicit
# invocation. No `--target` spelling merges a unit that also differs in features
# and profile. (This is the floor the R3 report below already warns about: "the
# host column can never reach zero".)
#
# The third IS a spelling pair, and W3's CMAKE half already retired it — the
# implicit member is residue from before that landed. So the axis the leaf half
# would move was already at zero here, and the tree it would move holds NO cargo
# leaf builds: every writer under it is corrosion (`cargo/<root>_<h>`, which
# hardcodes `--target`) or `nros_generate_interfaces()` glue.
#
# What DOES take this to 3 is R2 — collapsing the two corrosion roots (work-order
# item 5 / W2). They differ only in cargo's `path` field, repo-root-relative vs
# absolute, so merging the roots merges host with host and target with target:
# 5 -> 3. The number in the old prediction was right; the work item was not.
#
# **R2 RE-MEASURED 2026-08-10, and the sentence above is right about the field
# and wrong about the remedy.** `path` is not a spelling any caller picks: cargo
# spells a unit's source RELATIVE to the workspace root when the package is
# inside it and ABSOLUTE otherwise, so the field records a RELATION —
# `nano-ros_<h>` builds the shared crates as workspace MEMBERS
# (`packages/core/nros-serdes/src/lib.rs` in the dep-info), `nros_ws_runtime_<h>`
# reaches them as out-of-workspace PATH DEPS (the absolute spelling). Measured
# in a three-arm reproduction: an absolute and a relative `path =` dep line
# produce the SAME identity, and only member-vs-path-dep moves it. Cargo then
# closes the space from both sides — a build-dir workspace cannot adopt an
# in-repo crate ("member of the wrong workspace"; and with the root excluding
# it, "not hierarchically below the workspace root"), and the umbrella cannot
# join the repo-root workspace (an out-of-tree CMAKE_BINARY_DIR is not below it,
# and its lock names the user's node packages).
#
# So 5 -> 3 costs a corrosion ROOT, not a string: either issue 0493's
# single-provider design (delete root A), or moving nros-c/nros-cpp out of the
# repo-root workspace. Both are decisions with their own acceptance; until one
# lands this number is 5 and lowering it would be lying. Full evidence:
# phase-340, "R2 re-measured — `path` is a RELATION, not a spelling".
CEILING_IDENTITIES=5
CEILING_COPIES=5
# ----------------------------------------------------------------------------

# issue 0901 — `lane=native` was the advice and it does not build these trees.
# Measured: a full `just build-test-fixtures lane=native` (2 616 s) left every
# `examples/workspaces/*/build-workspace-fixtures*` untouched at its previous
# mtime. The workspace fixtures come from the WORKSPACE build, so name that.
# Advice that does not produce the artifact is worse than no advice: it costs
# the reader 40 minutes and leaves the gate exactly as silent as before.
BUILD_HINT="source ./activate.sh && just build-test-fixtures"

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
# issue 0499 — count only THIS build's artifacts.
#
# Cargo never collects the rlibs a previous build left, so on a long-lived tree
# the count grows with how many times it was built rather than with what the
# source says: a real regression and three days of history print the same
# message. Measured here twice — a tree reading 6 where a fresh rebuild read 5,
# and 10/11 where a clean tree read 4/6.
#
# The reference is `started_at`, the stamp's LOWER bound. NOT `built_at` and not
# the stamp file's mtime: both are written when the build SUCCEEDS, so every
# artifact the run produced is older than them, and filtering on either marks
# the whole current build as history — that version made this gate skip
# permanently, which is worse than over-counting because it reports green
# forever.
#
# Absence is "cannot filter", never "nothing is new": a stamp with no
# `started_at` (legacy, or hand-made) falls back to counting everything and the
# verdict says so, so an unfilterable reading is visible rather than silent.
# issue 0499 option 3 — close a failure with the DIAGNOSIS, not a request to
# guess. When the filter is active, accumulation is already excluded, so
# "delete the tree and rebuild" is wrong advice: it destroys the evidence of a
# real regression and re-measures to green. Only say it when the reading really
# could be history.
# When the reading REALLY could be history, name the cheap way to clear it.
#
# Rebuilding re-measures but does not remove the earlier eras — cargo never
# collects an artifact whose `-C metadata` identity it has moved past, so the
# same count comes back. Deleting the whole tree does work and costs a full
# rebuild. Pruning removes only the identities nothing references any more, so
# it costs nothing: the copy kept per slot is the one cargo links.
#
# Measured 2026-08-20 in the ROS distrobox: 509 superseded files / 1.52 GB, and
# `nros_core` went 12 identities -> 4 (its budget), with no rebuild afterwards.
_accumulation_hint() {
    echo "" >&2
    echo "  If it IS history, prune the superseded identities (no rebuild needed):" >&2
    echo "      python3 scripts/build/prune-superseded-artifacts.py $TREE          # dry run" >&2
    echo "      python3 scripts/build/prune-superseded-artifacts.py $TREE --apply" >&2
}

era_verdict() {
    # issue 0513 — `_era_filtered`, not `_started`: the filter can be ACTIVE and
    # still have been stepped around for this crate, and then "do not delete the
    # tree" is the wrong advice.
    if [ -n "${_started:-}" ] && [ "${_era_filtered:-1}" -eq 1 ]; then
        echo "  These are artifacts of THIS build (filtered on started_at=$_started)," >&2
        echo "  so accumulation is ruled out — the count is a real change, not history." >&2
        echo "  Do NOT delete the tree: that would erase the evidence and re-measure green." >&2
    elif [ -n "${_started:-}" ]; then
        # issue 0513 — the stamp HAS a started_at; this build simply did not
        # rebuild the budgeted crate, so the window could not speak for it and
        # the count above is unfiltered. Saying "no started_at" here sent the
        # reader looking for a missing stamp that is sitting right there.
        echo "  This build did not rebuild $BUDGET_CRATE, so the count above is" >&2
        echo "  UNFILTERED and MAY include earlier builds. Rebuild ($BUILD_HINT)" >&2
        echo "  and re-read before treating it as a regression." >&2
        _accumulation_hint
    else
        echo "  This stamp has no started_at, so the count MAY be accumulation from" >&2
        echo "  earlier builds. Rebuild ($BUILD_HINT) and re-read before believing it." >&2
        _accumulation_hint
    fi
}

STAMP="${NROS_FIXTURE_STAMP:-target/nextest/.fixtures-built}"
_all="$(find "$TREE" \
    -type d -path '*/out/sizes-probe-target-*' -prune -o \
    -path '*/deps/*' -name 'lib*-*.rlib' -print 2>/dev/null | sort)"

_started=""
_started_from_sidecar=0
[ -r "$STAMP" ] && _started="$(sed -n 's/^started_at=//p' "$STAMP" | head -1)"
# issue 0499 follow-up — the lower bound also lives OUTSIDE the stamp, and when
# the stamp is absent that is the only place it lives.
#
# `nros_fixtures_stamp_clear()` deletes the stamp and writes the build's start
# time to `<stamp>.started`; `nros_fixtures_stamp_write()` folds it in and
# removes it — ON SUCCESS. So a build that FAILS leaves no stamp and a surviving
# sidecar, and this gate then had no lower bound and counted the whole tree.
#
# That is not merely a worse number, it is an UNATTRIBUTABLE one, which is the
# thing this gate exists to avoid (see `era_verdict` and issue 0661): the count
# stops distinguishing "this build produced N identities" from "this tree
# collected N since June", and the advice flips between "find the axis" and
# "delete the tree".
#
# Worse, it WEDGES recovery. `build-test-fixtures` lists `check::fast` as a
# dependency, so this gate runs before the build that would write a good stamp —
# a single failed fixture build leaves every subsequent one blocked by a
# whole-tree count of history it did not create. Observed 2026-08-31 after a
# `lane=all` build failed on an unrelated compile error.
#
# The sidecar is a genuine lower bound: the pipeline wrote it, at the moment the
# build began. Reading it is strictly more correct than counting everything, and
# it is NOT the same as trusting a stamp — no `built_at`, no `lane=`, so nothing
# here claims a build SUCCEEDED. Only "the most recent attempt started then".
if [ -z "$_started" ] && [ -r "${STAMP}.started" ]; then
    _started="$(head -1 "${STAMP}.started")"
    [ -n "$_started" ] && _started_from_sidecar=1
fi
# The suffix goes into every message that quotes the bound. A reader who sees a
# filtered count must be able to tell WHICH build it is filtered against: a
# sidecar bound means the last attempt FAILED, so the artifacts inside the window
# are a partial set and "this build" means "what that attempt got through before
# it died". Silently identical wording for the two cases would be the issue-0661
# defect again — a correct number under advice that does not fit it.
_started_src=""
if [ "$_started_from_sidecar" = "1" ]; then
    _started_src=" (from ${STAMP}.started — the last build did NOT finish, so this window covers a PARTIAL build)"
fi
_ref=""
if [ -n "$_started" ]; then
    _ref="$(mktemp)"
    touch -d "$_started" "$_ref" 2>/dev/null || { rm -f "$_ref"; _ref=""; }
fi

if [ -n "$_ref" ] && [ -n "$_all" ]; then
    rlibs="$(printf '%s\n' "$_all" | while IFS= read -r f; do
        [ -n "$f" ] || continue
        [ "$f" -nt "$_ref" ] && printf '%s\n' "$f"
    done)"
    rm -f "$_ref"
    _n_all=$(printf '%s\n' "$_all" | grep -c . || true)
    _n_cur=$(printf '%s\n' "$rlibs" | grep -c . || true)
    if [ "$_n_cur" -eq 0 ]; then
        # Nothing newer than the build's own start: the tree predates the stamp
        # entirely. Say so instead of reporting a count about the past.
        echo "[SKIP] artifact-identity budget: all $_n_all rlib(s) in $TREE predate"
        echo "       started_at=$_started$_started_src — this tree is history, not that build."
        echo "       Rebuild to measure:  $BUILD_HINT"
        exit 0
    fi
    IDENTITY_ERA_NOTE="  counted $_n_cur of $_n_all rlib(s) — those written since started_at=$_started$_started_src (issue 0499)."
    # issue 0521-adjacent, filed under 0499: an INCREMENTAL build legitimately
    # rewrites nothing for a crate whose inputs did not change. The filter then
    # leaves artifacts for OTHER crates but none for the budget crate, and the
    # gate used to call that "a tree it did not understand" and fail — on a
    # correct build. It blocked tier 2 twice, each time "fixed" by deleting the
    # workspace and rebuilding, which is the wipe-to-green this issue's own
    # resolution argued against.
    #
    # The question ("how many identities of the budget crate does this tree
    # hold?") is still answerable — just not from THIS build's writes. Fall back
    # to the whole tree and label the reading unfiltered, which is exactly the
    # no-`started_at` case below and carries the same accumulation caveat.
    if [ -n "$rlibs" ] \
        && ! printf '%s\n' "$rlibs" | grep -q "/lib${BUDGET_CRATE}-" \
        && printf '%s\n' "$_all" | grep -q "/lib${BUDGET_CRATE}-"; then
        # issue 0647 — keep THIS build's writes for the tree-wide axes; only
        # the named-budget question needs the wider set (see the note there).
        _rlibs_era="$rlibs"
        rlibs="$_all"
        # issue 0661 — this reading is UNFILTERED, and the verdict has to know.
        #
        # `era_verdict()` picks its advice from `_era_filtered`. That flag was
        # initialised further down, AFTER this widening, and the later
        # 0647-aware widening only clears it when the crate is still missing —
        # which it is not, because this branch already widened. So a count that
        # this very note calls inflatable was announced as "accumulation is
        # ruled out — do NOT delete the tree".
        #
        # That is not a cosmetic mismatch: it is the advice that sent issue
        # 0661 hunting a cargo invocation that had skipped `--target`, when the
        # two no-`<triple>` artifacts it blamed are ordinary HOST
        # build-dependency output (cargo puts build-script deps in
        # `<dir>/<profile>/` precisely BECAUSE `--target` was passed) and the
        # real reading was two generations of target artifacts in one tree.
        _era_filtered=0
        IDENTITY_ERA_NOTE="  $BUDGET_CRATE was NOT rebuilt since started_at=$_started (an incremental
  build with nothing to do for it), so this counts ALL $_n_all rlib(s) in the
  tree — an accumulated tree can inflate it (issue 0499)."
    fi
else
    rlibs="$_all"
    IDENTITY_ERA_NOTE="  NO started_at in $STAMP and no ${STAMP}.started — counting all rlib(s); an accumulated tree inflates this (issue 0499)."
fi

if [ -z "$rlibs" ]; then
    echo "[SKIP] artifact-identity budget: $TREE holds no compiled rlibs"
    echo "       (configured but not built, or built for a non-Rust target)."
    echo "       To give it something to read:  $BUILD_HINT"
    exit 0
fi

# crate<space>hash<space>path, one per artifact.
triples="$(printf '%s\n' "$rlibs" \
    | sed -nE 's|^(.*/lib([A-Za-z0-9_]+)-([0-9a-f]{8,})\.rlib)$|\2 \3 \1|p')"
# issue 0647 — the era-only view, when widening replaced `rlibs` above.
if [ -n "${_rlibs_era:-}" ]; then
    triples_era="$(printf '%s\n' "$_rlibs_era" \
        | sed -nE 's|^(.*/lib([A-Za-z0-9_]+)-([0-9a-f]{8,})\.rlib)$|\2 \3 \1|p')"
fi

if [ -z "$triples" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "$IDENTITY_ERA_NOTE" >&2
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
#
# Counted in ONE awk pass, deliberately: the `sort -u | awk | uniq -c` pipeline
# this replaces was WRONG under any non-C locale, and had been since the gate
# landed (found 2026-08-10, phase-340 item 8).
#
# `uniq -c` collapses only ADJACENT duplicates, and glibc's en_US.UTF-8
# collation ignores the space and the underscore when ordering, so
#
#     nros 079babbedb254517        collates as  nros079babbedb254517
#     nros_board_common 2f72d54…   collates as  nrosboardcommon2f72d54…
#     nros ecf7643749b10a78        collates as  nrosecf7643749b10a78
#
# put `nros_board_common`, `nros_core` and `nros_cpp` BETWEEN two halves of
# `nros`. The crate then appeared twice in `identity_counts` — as 7 and as 5 —
# and every consumer read one run as the whole crate:
#
#   * the tree-wide ceiling (`$1 > CEILING_IDENTITIES`) compared 7 and 5
#     separately, so `nros` at **12** identities passed a ceiling of 9;
#   * the headline "worst crate N/9" under-reported for the same reason;
#   * `crate_identities` would have emitted TWO lines for a split crate, and
#     `[ "$n" -gt "$k" ]` on a two-line value is a bash syntax error — the
#     budgeted crate `nros_core` is contiguous only because its four hashes all
#     start 0/4/6/9. One starting with `e` or `f` would have split it and taken
#     the gate down.
#
# This is the phase's own standing rule turned on the gate that enforces it:
# "re-measure an N of M claim before building on it". Item 8 was blocked on
# explaining a `worst crate` figure that moved 5 -> 6 -> 7 across sessions on an
# ostensibly unchanged tree. It was never drift. It was the RUN BOUNDARY moving
# as hashes changed.
#
# An associative array over (crate, hash) has no ordering to get wrong.
count_identities() {
    # stdin: `crate hash [path]` lines.  stdout: `<n> <crate>`, one per crate.
    awk '
        { if (!seen[$1 SUBSEP $2]++) n[$1]++ }
        END { for (c in n) print n[c], c }'
}

# The counter is SELF-TESTED on every run, against input engineered to split
# under glibc collation but not under C: `nros 0…`, `nros_board 1…`, `nros f…`
# collate as `nros0…` < `nrosboard1…` < `nrosf…`, so the two `nros` rows are
# non-adjacent exactly the way the real tree's were.
#
# Standing, not a one-off, because the bug is INVISIBLE in output: the old
# pipeline printed a plausible smaller number and exited 0. Nothing about a
# wrong reading looks wrong. It cost this phase a blocked work item chasing a
# "drifting" figure that was only ever the run boundary moving.
_selftest="$(printf 'nros 0aaaaaaaa\nnros_board 1bbbbbbbb\nnros fccccccccc\n' \
    | count_identities | awk '$2 == "nros" {print $1}')"
if [ "$_selftest" != "2" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "$IDENTITY_ERA_NOTE" >&2
    echo "  the identity counter is not collation-independent: it reported" >&2
    echo "  '$_selftest' identities for a crate that has exactly 2." >&2
    echo "  A counter that splits one crate into two runs under-reports the" >&2
    echo "  worst-crate figure and lets a crate over the ceiling pass — it did," >&2
    echo "  for `nros` at 12 against a ceiling of 9, until 2026-08-10." >&2
    exit 1
fi

# issue 0513 — SELF-TEST the fallback predicate, standing like the counter's.
#
# The bug it guards is invisible in output: with the budgeted crate absent from
# the window the gate printed a confident, wrong "NONE for nros_core" and exited
# 1 on a correct tree. A predicate that silently stops firing would restore that
# without looking different.
_fb_present="$(printf '3 nros_core\n1 winnow\n' \
    | awk -v c="nros_core" '$2 == c {found=1} END {exit !found}' && echo yes || echo no)"
_fb_absent="$(printf '1 winnow\n' \
    | awk -v c="nros_core" '$2 == c {found=1} END {exit !found}' && echo yes || echo no)"
if [ "$_fb_present" != "yes" ] || [ "$_fb_absent" != "no" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "  the issue-0513 fallback predicate is broken: present='$_fb_present'" >&2
    echo "  (want yes), absent='$_fb_absent' (want no). With it wrong, an" >&2
    echo "  incremental build that did not rebuild $BUDGET_CRATE either hard-fails" >&2
    echo "  a correct tree or silently skips the whole measurement." >&2
    exit 1
fi

identity_counts="$(printf '%s\n' "$triples" | count_identities)"

# issue 0513 — an INCREMENTAL build does not rewrite what it did not rebuild.
#
# The era filter (0499) answers "what did THIS build produce". That is the right
# question for accumulation and the wrong one for "how many identities does this
# crate have": cargo leaves an untouched rlib exactly where it was, so a run
# whose diff never reaches $BUDGET_CRATE leaves ZERO of its artifacts in the
# window while the tree holds a complete, correct set of them.
#
# 0499 already handles the all-history case (every rlib predates the stamp ->
# SKIP). This is the PARTIAL one: some crates rebuilt, the budgeted crate did
# not. It reached the "NONE for $BUDGET_CRATE" arm — written for a partial build
# or a renamed crate — and hard-failed the first member of `check-fast`, which
# stops `ci` before the build tier, clippy and `test-all`. Observed: 16 of 244
# rlibs in the window, four `nros_core` rlibs in the tree from the build 50
# minutes earlier.
#
# So: when the window is non-empty but says nothing about the budgeted crate,
# measure the WHOLE tree and label the reading as possibly-historic. That can
# only count MORE, never fewer, so it introduces no false green — an over-budget
# crate is still reported, with the caveat and the rebuild remedy that a
# stampless tree already gets.
# issue 0661 — do NOT clobber a 0 set by the earlier widening above.
_era_filtered="${_era_filtered:-1}"
if [ -n "${_started:-}" ] && [ -n "$_all" ] \
    && ! printf '%s\n' "$identity_counts" | awk -v c="$BUDGET_CRATE" '$2 == c {found=1} END {exit !found}'; then
    _all_triples="$(printf '%s\n' "$_all" \
        | sed -nE 's|^(.*/lib([A-Za-z0-9_]+)-([0-9a-f]{8,})\.rlib)$|\2 \3 \1|p')"
    if printf '%s\n' "$_all_triples" | awk -v c="$BUDGET_CRATE" '$1 == c {found=1} END {exit !found}'; then
        # issue 0647 — widening answers the NAMED-BUDGET question only.
        #
        # The two tree-wide axes below ask a different one: "how many identities
        # / copies did THIS BUILD produce?" Answering that from an accumulated
        # tree is a false red, and a routine one — a clean build of the mixed
        # workspace lands exactly ON the ceiling (5/5), so the first incremental
        # rebuild that changes any fingerprint puts a crate at 6 and fails the
        # gate, with `rm -rf` + a 7-minute rebuild as the only remedy. Hit twice
        # in one session, both times on a correct tree.
        #
        # Keeping the era set for those axes cannot create a false GREEN: a
        # build that really compiles six units of a crate writes all six INSIDE
        # the window. What it drops is crates this build never compiled, which
        # is exactly what it has nothing to say about.
        triples_era="$triples"
        triples="$_all_triples"
        identity_counts="$(printf '%s\n' "$triples" | count_identities)"
        _era_filtered=0
        IDENTITY_ERA_NOTE="  counted ALL $_n_all rlib(s): this build rebuilt $_n_cur of them and none for $BUDGET_CRATE, so the started_at window says nothing about it (issue 0513). The count MAY include earlier builds."
    fi
fi

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
    echo "$IDENTITY_ERA_NOTE" >&2
    echo "  $TREE holds compiled rlibs, but NONE for $BUDGET_CRATE." >&2
    echo "  The gate cannot answer the question it exists to ask, so it fails" >&2
    echo "  rather than passing on a tree it did not understand. Either the" >&2
    echo "  build is partial (rebuild: $BUILD_HINT) or the crate was renamed," >&2
    echo "  in which case update BUDGET_CRATE in $0." >&2
    exit 1
fi

if [ "$budgeted_n" -gt "$BUDGET_IDENTITIES" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "$IDENTITY_ERA_NOTE" >&2
    echo "  $BUDGET_CRATE has $budgeted_n distinct -C metadata identities in $TREE" >&2
    echo "  (budget $BUDGET_IDENTITIES, recorded 2026-08-07 by phase-340 W4)." >&2
    echo "  Each identity is a separate compilation of the same crate:" >&2
    report_crate "$BUDGET_CRATE" >&2
    axis_report >&2
    echo "  A new identity means a new incompatibility axis — workspace root," >&2
    echo "  explicit --target, RUSTFLAGS, or any ONE of opt-level /" >&2
    echo "  debug-assertions / panic / codegen-units / lto / incremental." >&2
    echo "  Target dir is NOT one of them. See phase-340 'The complete" >&2
    echo "  incompatibility set'." >&2
    era_verdict
    fail=1
fi

# issue 0647 — the tree-wide axes read THIS BUILD's writes whenever a window
# exists, even when the named-budget crate forced the widening above.
_tree_triples="${triples_era:-$triples}"
_tree_counts="$(printf '%s\n' "$_tree_triples" | count_identities)"
if [ -n "${triples_era:-}" ]; then
    _dropped=$(printf '%s\n' "$triples" | awk '{print $1}' | sort -u | wc -l)
    _kept=$(printf '%s\n' "$_tree_triples" | awk '{print $1}' | sort -u | wc -l)
    echo "  tree-wide axes read the $_n_cur rlib(s) THIS build wrote ($_kept crate(s));" \
         "$((_dropped - _kept)) crate(s) it did not compile are not judged here (issue 0647)."
fi

over_ceiling="$(printf '%s\n' "$_tree_counts" \
    | awk -v k="$CEILING_IDENTITIES" '$1 > k {print $2, $1}')"
if [ -n "$over_ceiling" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "$IDENTITY_ERA_NOTE" >&2
    echo "  crates over the tree-wide ceiling of $CEILING_IDENTITIES identities in $TREE:" >&2
    while read -r crate n; do
        echo "  $crate: $n" >&2
        report_crate "$crate" >&2
    done <<< "$over_ceiling"
    echo "  Read the PATHS above before reading this as a regression. An" >&2
    echo "  identity under a \`nano_ros_cpp_ffi_*/target/<profile>/deps\` with NO" >&2
    echo "  <triple> component is pre-2026-08-08 residue: the cmake lane cannot" >&2
    echo "  emit that spelling any more (phase-340 W3), so a long-lived tree can" >&2
    echo "  carry a compilation nothing rebuilds. Delete $TREE and rebuild" >&2
    echo "  before believing the count." >&2
    fail=1
fi

# --- axis 2: copies of ONE identity (R1 — per-directory isolation) ----------
# `sort | uniq -c` IS correct here, unlike axis 1 above: this counts IDENTICAL
# lines (one crate+hash pair against itself), and identical lines are adjacent
# after any sort, in any locale. The axis-1 bug needed two DIFFERENT lines to be
# reduced to a common key first. Left as a pipeline on purpose, with the reason
# stated, so nobody "fixes" a correct site — or copies the broken idiom.
over_copies="$(printf '%s\n' "$_tree_triples" | awk '{print $1, $2}' | sort | uniq -c \
    | awk -v k="$CEILING_COPIES" '$1 > k {print $2, $3, $1}')"
if [ -n "$over_copies" ]; then
    echo "artifact-identity budget: FAIL" >&2
    echo "$IDENTITY_ERA_NOTE" >&2
    echo "  identities written into more than $CEILING_COPIES target dirs in $TREE:" >&2
    while read -r crate hash n; do
        echo "  $crate $hash: $n copies" >&2
        printf '%s\n' "$_tree_triples" | awk -v h="$hash" '$2 == h {print "    " $3}' >&2
    done <<< "$over_copies"
    echo "  Cargo already judged these interchangeable; they are the same" >&2
    echo "  compilation repeated per directory (phase-340 R1)." >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    exit 1
fi

max_ids="$(printf '%s\n' "$_tree_counts" | awk '{print $1}' | sort -rn | head -1)"
max_copies="$(printf '%s\n' "$_tree_triples" | awk '{print $1, $2}' | sort | uniq -c \
    | awk '{print $1}' | sort -rn | head -1)"
echo "artifact-identity budget OK ($TREE): $BUDGET_CRATE $budgeted_n/$BUDGET_IDENTITIES identities;" \
     "worst crate $max_ids/$CEILING_IDENTITIES; worst identity $max_copies/$CEILING_COPIES copies."
echo "$IDENTITY_ERA_NOTE"
axis_report
