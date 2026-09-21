#!/usr/bin/env bash
#
# Issue 0359 — stop leaf `Cargo.lock` drift from growing silently.
#
# 48 tracked leaf lockfiles live outside the root workspace (board crates,
# drivers, standalone bins). Nothing ever ran `--locked` over them, so when a
# manifest gained a dependency the lock was simply never regenerated, and the
# drift grew with every edit. It surfaced only when somebody happened to build
# one — which is how 0359 was found, during phase-318 acceptance.
#
# The consequence is worse than staleness: a lock that cannot satisfy its own
# manifest is NOT PINNING ANYTHING. Those leaves resolve fresh on every build,
# so two developers on the same commit can compile different dependency
# versions. That is issue 0182's class one layer out — a committed artifact that
# looks authoritative and is not consulted. It matters most on embedded targets,
# and 12 of the 18 registry-affecting cases are board or driver crates.
#
# WHY A BASELINE RATHER THAN A HARD FAIL
#
# 27 leaves are drifted today and cannot be fixed by regenerating: the manifests
# GREW, so `nros-board-nuttx-qemu` alone comes back with 86 packages added
# and 0 removed. Regenerating pins 86 registry crates at whatever resolves that
# minute, on embedded targets — a supply-chain decision, not a cleanup, and
# deliberately out of scope here (see 0359 for the pinning options).
#
# So this gate freezes the known-bad set and fails on CHANGE in either
# direction:
#
#   * a leaf drifts that is NOT baselined  -> new drift, the thing we are here
#     to stop.
#   * a baselined leaf stops drifting      -> the baseline is stale; delete the
#     line. This is what forces the list to SHRINK as 0359 is worked, instead of
#     becoming a dumping ground nobody ever revisits.
#
# A gate that cannot pass gets bypassed, and a bypassed gate is worth less than
# no gate — hence the baseline rather than 27 red lines on day one.

set -euo pipefail
cd "$(dirname "$0")/.."

# issue 1184 — the unsynced-leaf SKIP below has to be COUNTABLE, not just
# printed. It writes to stderr and returns 0, so `check-fast`'s "N SKIPPED"
# summary never saw it and CI read a clean pass over two leaves nobody checked.
# shellcheck source=scripts/build/check-skip.sh
. "$(pwd)/scripts/build/check-skip.sh"

# issue 0726 — the leaf classification below is three `grep -qE` arms in a
# chain, and a grep that failed to run falls through to the `else` and files the
# leaf as BROKEN with three lines of cargo output attached. That is a confident,
# specific, wrong verdict about a leaf that is fine. `nros_grep_q` exits 2.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

BASELINE="scripts/leaf-lockfile-drift-baseline.txt"

# --- INVARIANT: a lock is tracked only where it can resolve from a fresh clone
#
# RFC-0067 / RFC-0026. A committed lock names every package in the graph, so it
# is only meaningful if a `git clone` can produce that graph. Two classes can:
#
#   * leaves with NO message deps (boards, drivers, smoke, verification), and
#   * leaves whose message deps resolve to a COMMITTED `generated/` tree —
#     including the core `packages/interfaces/*` crates, which are pre-generated
#     under `nros-` prefixed names precisely so they exist before any user
#     codegen and cannot collide with a user's own msg packages.
#
# A leaf whose `generated/` tree is produced by the USER (`nros sync`, from THEIR
# ament packages) cannot: at clone time those crates do not exist, so the lock
# names phantoms and every cargo command in that leaf fails. Ten such locks were
# tracked until 2026-08-03; `stress-zenoh` and friends were the leaves that made
# `just build-test-fixtures` unrunnable on a fresh host.
#
# So: tracked lock  <=>  (no message deps) OR (generated/ committed).
# This check is the enforcement; deleting the lock is the fix, never adding a
# committed `generated/` tree to satisfy it — that tree belongs to the user.
check_lock_tracking_invariant() {
    local status=0 leaf gen msgdeps
    while IFS= read -r lock; do
        leaf="$(dirname "$lock")"
        # committed generated/ anywhere under the leaf (incl. workspace members)
        gen="$(git ls-files "$leaf/generated" "$leaf/src/*/generated" | head -1)"
        [ -n "$gen" ] && continue
        # message deps declared by the leaf or any of its workspace members
        # -h, NOT -rh: every argument here is an explicit file path, so `-r`
        # never recursed — it only tripped `check-no-tracked-file-find`, which
        # reads the flag and not the arguments (correctly: the flag is the part
        # that turns into a walk the day someone passes a directory).
        msgdeps="$(grep -hE "^($MSG_CRATES) = " \
            "$leaf/Cargo.toml" "$leaf"/src/*/Cargo.toml 2>/dev/null | wc -l)"
        [ "$msgdeps" -eq 0 ] && continue
        echo "FAIL: $lock is tracked, but this leaf's message deps come from a" >&2
        echo "      USER-side generated/ tree ($msgdeps dep(s), nothing committed)." >&2
        echo "      A fresh clone cannot resolve that lock. Fix: git rm --cached" >&2
        echo "      $lock  and add /Cargo.lock to the leaf .gitignore (RFC-0067)." >&2
        status=1
    done < <(git ls-files '*/Cargo.lock')
    return $status
}

MSG_CRATES='std_msgs|builtin_interfaces|example_interfaces|geometry_msgs|sensor_msgs|lifecycle_msgs|action_msgs|rosgraph_msgs|nav_msgs|diagnostic_msgs|trajectory_msgs|shape_msgs|stereo_msgs|visualization_msgs|unique_identifier_msgs|test_msgs'

if ! check_lock_tracking_invariant; then
    echo "" >&2
    echo "check-leaf-lockfiles: lock-tracking invariant violated (see above)." >&2
    exit 1
fi

# NOT `--offline`, deliberately — the first version of this gate used it and was
# wrong. Offline conflates two different things: "this lock cannot satisfy its
# manifest" and "this crate is not in the local cargo cache". Pinning the 26
# backlog leaves immediately exposed it — eleven of them reported
# `failed to download cortex-m-rt v0.7.6` purely because the newly pinned
# version had never been fetched on this machine. On a cold CI cache that is
# EVERY leaf, so the gate would have been red for a reason that has nothing to
# do with lockfiles. CI downloads crates for every build anyway, so allowing the
# fetch costs nothing real.
#
# Match that message specifically rather than treating ANY non-zero exit as
# drift: a missing vendored dependency or a broken manifest must not be
# misreported as lock drift, or the gate teaches people the wrong fix.
DRIFT_RE='cannot update the lock file .* because --locked was passed'

# Leaves that cannot resolve STANDALONE by design, so `--locked` says nothing
# about their lock. `tests/simple-workspace` ships no `.cargo/config.toml`: its
# `nros-core` dep is registry-style and only resolves once `nros sync` writes
# the patch table, so cargo searches crates.io and never finds it. Skipping it is
# honest; classifying it as drift would be a lie, and
# leaving it as "broken" would make the gate permanently red.
SKIP_RE='^(tests/simple-workspace)$'

# Issue 0378 — an UNSYNCED tree, told apart from a broken one.
#
# The leaf `.cargo/config.toml` is `nros sync`-managed (RFC-0048): it includes
# the central, gitignored `nros-patch.toml` and redirects the message crates at
# a per-leaf `generated/` tree that `nros generate-rust` produces. On a host set
# up by the BOOK's flow (bootstrap + `nros setup`) neither exists yet, so cargo
# cannot even load the config.
#
# That is not a broken crate and not lock drift — it is a setup step nobody was
# told to run. Before this, nine leaves landed in the unclassified bucket and
# the gate printed "failed for a reason that is NOT lock drift" with no remedy,
# which is where issue 0378's reporter got stuck.
#
# Both spellings fail CLOSED (verified, not assumed): a missing `include`
# aborts config loading outright, and a present patch pointing at an absent
# path fails to load the source. Neither silently falls through to crates.io.
UNSYNCED_RE='failed to load config include|failed to read configuration file .*nros-patch\.toml|unable to update .*/generated/|failed to load source for dependency'

# Issue 1398 — a SUBMODULE that is not checked out, told apart from both.
#
# The branch below reads the central `nros-patch.toml` as proof that the tree is
# provisioned, and it is not: `nros sync` writes patch tables and `generated/`
# trees, and it does not fetch submodules. A tree can be fully synced with no
# submodule checked out at all.
#
# Two leaves prove it. `nros-nuttx-ffi` and `nros-nuttx-riscv-ffi` carry, in the
# TRACKED half of their `.cargo/config.toml` (authored, not sync-managed, so
# present in a fresh clone):
#
#     [patch.crates-io]
#     libc = { path = "../../../../third-party/nuttx/libc" }
#
# With `third-party/nuttx/libc` absent, cargo says `failed to load source for
# dependency libc` — which `UNSYNCED_RE` matches — and the patch-table branch
# then reported "their patch tables or `generated/` trees are wrong" and sent
# the reader to `nros sync`, which cannot fetch a submodule and, run at the repo
# root, refuses outright.
#
# We check the PATH DEP TARGETS rather than the wording of cargo's error, for
# the same reason `DRIFT_RE` is specific rather than "any non-zero exit": a
# message is a thing upstream can change, a missing `Cargo.toml` is not. And we
# require the target to be a DECLARED SUBMODULE (`.gitmodules`), because that —
# not "it is under `third-party/`" — is what makes it provisioning rather than a
# defect: a path dep pointing at a directory nobody can check out is a broken
# tree and must stay a hard failure.
#
# `_nros_submodule_roots_abs` is a newline-separated list of absolute submodule
# roots. It is a variable, not a call, so the selftest can substitute a set it
# controls without touching the repo.
_nros_submodule_roots_abs=""
load_submodule_roots() {
    local rel roots=""
    while IFS= read -r rel; do
        [ -n "$rel" ] || continue
        roots+="$(realpath -m "$rel")"$'\n'
    done < <(git config -f .gitmodules --get-regexp '^submodule\..*\.path$' 2>/dev/null \
        | awk '{print $2}')
    _nros_submodule_roots_abs="$roots"
}

# Every `path = "…"` a leaf declares, in the two files that can carry one: its
# manifest and its cargo config. Both resolve relative to the LEAF DIRECTORY
# (verified against cargo's own error, which named the fully resolved path).
leaf_path_deps() {
    local dir="$1"
    sed -n \
        -e 's/^[[:space:]]*path[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' \
        -e 's/.*[{,][[:space:]]*path[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' \
        "$dir/.cargo/config.toml" "$dir/Cargo.toml" 2>/dev/null | sort -u
}

# `<abs>` is at or under a declared submodule root that is NOT CHECKED OUT ->
# print that root.
#
# The emptiness test is what keeps the widening honest. Containment alone would
# also swallow a dep pointing INSIDE a checked-out submodule at a directory that
# is missing — a broken tree wearing a provisioning gap's clothes, and a silent
# skip where the hard failure belongs. `git submodule deinit` leaves the
# directory present and empty, so "empty or absent" is exactly git's own
# not-checked-out.
submodule_root_for() {
    local abs="$1" root
    while IFS= read -r root; do
        [ -n "$root" ] || continue
        [ "$abs" = "$root" ] || [ "${abs#"$root"/}" != "$abs" ] || continue
        [ -z "$(ls -A "$root" 2>/dev/null)" ] || continue
        printf '%s\n' "$root"
        return 0
    done <<<"$_nros_submodule_roots_abs"
    return 1
}

# Prints `<spec>|<repo-relative submodule root>` per unprovisioned path dep;
# returns 0 if there was at least one. A target that EXISTS as a crate is never
# reported, so a leaf that still fails on a provisioned tree falls through to
# the ordinary classification and its hard failure.
unprovisioned_submodule_deps() {
    local dir="$1" found=1 spec abs root
    while IFS= read -r spec; do
        [ -n "$spec" ] || continue
        case "$spec" in
            /*) abs="$spec" ;;
            *)  abs="$dir/$spec" ;;
        esac
        abs="$(realpath -m "$abs")"
        [ -f "$abs/Cargo.toml" ] && continue
        root="$(submodule_root_for "$abs")" || continue
        printf '%s|%s\n' "$spec" "$(realpath -m --relative-to=. "$root")"
        found=0
    done < <(leaf_path_deps "$dir")
    return "$found"
}

# Issue 0378 — drift that is ENVIRONMENT, not a dependency change.
#
# Generated message crates take their version from the consumer's ament install
# and are never shipped. Cargo still records them in `Cargo.lock` (it records
# every resolved package, path deps included), so a committed lock ends up
# asserting which ROS install produced it and every other install reads as
# drift. Regenerating "fixes" it only for the host that regenerated — that is
# how `action_msgs 1.2.3 -> 1.2.2` got committed and then reverted.
#
# So: if a leaf's drift is only that its generated msg crates carry a different
# version than the lock, say so and do NOT demand a pointless refresh. The real
# fix is normalising the generated version (codegen now emits 0.0.0), after
# which this branch stops firing.
msg_version_drift_only() {
    local dir="$1" lock="$1/Cargo.lock" m n gv lv found=1
    [ -d "$dir/generated" ] || return 1
    for m in "$dir"/generated/*/; do
        [ -d "$m" ] || continue
        n="$(basename "$m")"
        # `|| true` — a generated crate with no `version` line is what the
        # `[ -z ]` below skips (issue 1249).
        gv="$(grep -m1 '^version' "$m/Cargo.toml" 2>/dev/null | cut -d'"' -f2 || true)"
        [ -z "$gv" ] && continue
        lv="$(awk -v n="$n" '/^\[\[package\]\]/{p=0} $0=="name = \""n"\""{p=1} p&&/^version/{gsub(/"/,"",$3); print $3; exit}' "$lock" 2>/dev/null)"
        [ -z "$lv" ] && continue
        if [ "$gv" != "$lv" ]; then
            echo "    $n: generated=$gv lock=$lv"
            found=0
        fi
    done
    return "$found"
}

# Negative control for the issue-1398 classifier, run on the NORMAL path.
#
# The two directions have to be demonstrated together, because each alone is
# satisfied by a broken predicate: a probe that always says "unprovisioned"
# turns the gate into a no-op, and one that never does restores 1398. So:
#
#   A. an absent submodule reads as a provisioning gap (the bug), and
#   B. the same leaf with the target checked out does NOT (so a genuinely
#      broken leaf on a provisioned tree still reaches the hard failure), and
#   C. a path dep at an absent directory that is NOT a declared submodule does
#      NOT either — that is a broken tree, not a missing checkout, and
#   D. neither does one pointing INSIDE a submodule that IS checked out, at a
#      directory that is missing. Same reason as C, and the arm containment
#      alone would get wrong.
#
# Pure: it builds directories and files, never a repository, so it cannot hit
# issue 0986 (a `git init` under an inherited `GIT_DIR` writes into the CALLER's
# repo — from a `pre-push` hook, which this gate's lane runs in).
self_test() {
    local tmp saved root rc=0 out
    saved="$_nros_submodule_roots_abs"
    # Same scratch rule as the drift temporaries below: `$repo/tmp/`, which the
    # repo already ignores, never the shared system `/tmp`.
    root="$(git rev-parse --show-toplevel)/tmp"
    mkdir -p "$root"
    tmp="$(mktemp -d "$root/leaf-selftest.XXXXXX")"

    mkdir -p "$tmp/leaf/.cargo"
    printf '[package]\nname = "leaf"\nversion = "0.0.0"\n' > "$tmp/leaf/Cargo.toml"
    printf '%s\n' '[patch.crates-io]' \
        'libc = { path = "../sub/libc" }' \
        'other = { path = "../plain/dir" }' \
        'inner = { path = "../sub/libc/gone" }' \
        > "$tmp/leaf/.cargo/config.toml"
    # `../sub/libc` is a declared submodule; `../plain/dir` is not. The dir
    # exists and is EMPTY — `git submodule deinit` leaves exactly that.
    mkdir -p "$tmp/sub/libc"
    _nros_submodule_roots_abs="$tmp/sub/libc"$'\n'

    # A — the submodule is not checked out.
    if out="$(unprovisioned_submodule_deps "$tmp/leaf")"; then
        case "$out" in
            *"sub/libc"*) ;;
            *) echo "[FAIL] selftest A: an absent submodule was detected, but the" >&2
               echo "       report does not name it: $out" >&2
               rc=1 ;;
        esac
        case "$out" in
            *"plain/dir"*)
                echo "[FAIL] selftest C: an absent path dep that is NOT a declared" >&2
                echo "       submodule was reported as a provisioning gap. That is a" >&2
                echo "       broken tree and must stay a hard failure." >&2
                rc=1 ;;
        esac
    else
        echo "[FAIL] selftest A: a leaf path-depending on a submodule that is NOT" >&2
        echo "       checked out was not classified as a provisioning gap — issue" >&2
        echo "       1398 would be back, and it reports as two broken leaves." >&2
        rc=1
    fi

    # B and D — the same leaf, submodule checked out. Neither `../sub/libc`
    # (which now resolves) nor `../sub/libc/gone` (which does not, but is inside
    # a provisioned submodule) may be reported.
    printf '[package]\nname = "libc"\nversion = "0.0.0"\n' > "$tmp/sub/libc/Cargo.toml"
    if out="$(unprovisioned_submodule_deps "$tmp/leaf")"; then
        echo "[FAIL] selftest B/D: a leaf whose submodule IS checked out was still" >&2
        echo "       called a provisioning gap ($out). A leaf that fails on a" >&2
        echo "       provisioned tree must reach the hard failure, or this gate" >&2
        echo "       stops asserting anything." >&2
        rc=1
    fi

    rm -rf "$tmp"
    _nros_submodule_roots_abs="$saved"
    return "$rc"
}

load_submodule_roots
self_test || exit 1

drifted=()
broken=()
unsynced=()
unprovisioned=()
unprovisioned_detail=()
msg_drift=()
while read -r lock; do
    dir="$(dirname "$lock")"
    if nros_grep_q -E "$SKIP_RE" <<<"$dir"; then
        continue
    fi
    if out="$( cd "$dir" && cargo metadata --locked --format-version 1 2>&1 >/dev/null )"; then
        continue
    fi
    # Issue 1398 — FIRST, because it is the only arm that is about the
    # environment rather than the tree, and cargo reports an unloadable source
    # before it reports anything else. Its answer does not depend on the error
    # text, so it is stable under a cargo that rewords one.
    if detail="$(unprovisioned_submodule_deps "$dir")"; then
        unprovisioned+=("$dir")
        while IFS= read -r line; do
            [ -n "$line" ] && unprovisioned_detail+=("$dir|$line")
        done <<<"$detail"
        continue
    fi
    if nros_grep_q -E "$DRIFT_RE" <<<"$out"; then
        if detail="$(msg_version_drift_only "$dir")"; then
            msg_drift+=("$dir")
            printf '%s\n' "$detail" >/dev/null
            continue
        fi
        drifted+=("$dir")
    elif nros_grep_q -E "$UNSYNCED_RE" <<<"$out"; then
        unsynced+=("$dir")
    else
        broken+=("$dir")
        printf '  %s\n' "$dir" >&2
        printf '%s\n' "$out" | head -3 | sed 's/^/      /' >&2
    fi
done < <(git ls-files '*/Cargo.lock' | grep -v '^third-party/' | grep -v '^packages/cli/')

if [ ${#unprovisioned[@]} -gt 0 ]; then
    # Issue 1398 — the THIRD outcome (issue 1043's vocabulary): not OK, not
    # FAIL, but NOT VERIFIED. These leaves path-depend on a submodule that is
    # not checked out, so `--locked` cannot say anything about their locks, and
    # no amount of correctness in the tree would change that.
    #
    # RECORDED, not merely printed (issue 1184): a line on stderr is not
    # countable, and the lane summary would go on claiming these were checked.
    # And exit 0, for issue 0466's reason — this gate is on `check-fast`, which
    # `pre-push` runs, and a hook must not refuse a push over a checkout the
    # lane is not specified to provide.
    mapfile -t _missing_roots < <(printf '%s\n' "${unprovisioned_detail[@]}" \
        | cut -d'|' -f3 | sort -u)
    nros_check_skip leaf-lockfiles \
        "submodule(s) not checked out — ${#unprovisioned[@]} leaf crate(s) NOT checked: ${unprovisioned[*]}"
    for _d in "${unprovisioned_detail[@]}"; do
        IFS='|' read -r _leaf _spec _root <<<"$_d"
        printf '       %s\n           path dep %s -> submodule %s (not checked out)\n' \
            "$_leaf" "$_spec" "$_root" >&2
    done
    echo "" >&2
    echo "       Those rows are in the TRACKED half of the leaf's cargo config, so" >&2
    echo "       they are there in a fresh clone — but the submodule they point at" >&2
    echo "       is not. \`nros sync\` does NOT fetch submodules, so a synced tree" >&2
    echo "       reaches this state too (issue 1398). Fetch them:" >&2
    echo "" >&2
    printf '           git submodule update --init --depth 1 %s\n' "${_missing_roots[*]}" >&2
    echo "" >&2
    echo "       Every OTHER leaf lock was still checked." >&2
fi

if [ ${#unsynced[@]} -gt 0 ]; then
    # Is the TREE unsynced, or is this leaf broken on a synced tree? The central
    # `nros-patch.toml` answers it: `nros sync` writes it, `.gitignore` excludes
    # it, so a fresh clone never has one.
    #
    # Issue 0466 — this used to `exit 1` either way, which made the gate
    # unrunnable in the very lane it lives in. `check-fast` is documented
    # BUILDLESS *and* SOURCE-FREE, and gate.yml deliberately does NOT
    # provision the CLI or run `nros sync` on a push; so on every push this gate
    # met two leaves it could not resolve and failed. Per-push CI was red
    # CONTINUOUSLY for over a day on exactly this, which also buried every other
    # signal that lane exists to give.
    #
    # Not-synced is a statement about the ENVIRONMENT, not about the tree's
    # correctness, and a gate cannot demand setup the lane is specified not to
    # do. So: warn and keep going, still gating every leaf that DID resolve.
    # With the central patch present the tree IS synced, an unresolvable leaf is
    # then a real defect, and the old hard failure stands unchanged.
    #
    # Issue 1398 — "the central patch table exists" now means only what it can
    # mean: `nros sync` ran. It says nothing about SUBMODULES, which sync does
    # not fetch, so those leaves are classified above and never reach here. What
    # is left is the sync-shaped gap this branch was written for, and the hard
    # failure is sound for it.
    if [ -f "nros-patch.toml" ]; then
        echo "ERROR: ${#unsynced[@]} leaf crate(s) cannot resolve on a SYNCED tree." >&2
        printf '       %s\n' "${unsynced[@]}" >&2
        echo "" >&2
        echo "       \`nros-patch.toml\` exists, so \`nros sync\` has run here, and no" >&2
        echo "       unprovisioned submodule explains these (that is checked first," >&2
        echo "       issue 1398). Re-run \`nros sync\`; if they persist, their patch" >&2
        echo "       tables or \`generated/\` trees are wrong." >&2
        exit 1
    fi
    # issue 1184 — RECORD it, not just print it. Every CI checkout is unsynced
    # (`nros sync` writes `nros-patch.toml` and the per-leaf `generated/` trees,
    # neither committed), so this branch is the one CI always takes, and it
    # exited 0 with nothing countable. Two leaves drifted for a whole phase
    # behind it: `nros-macros` gained `nros-entry-lower` and both nuttx-ffi
    # locks went stale, invisible to every unsynced tree including CI, and
    # visible immediately on a synced one.
    #
    # Still a SKIP rather than a failure: these leaves genuinely cannot resolve
    # without sync (issue 0378), so failing closed would break the book's own
    # install flow. The ledger is what makes the difference between "checked"
    # and "not checked" survive into the lane summary.
    nros_check_skip leaf-lockfiles \
        "tree not synced — ${#unsynced[@]} leaf crate(s) NOT checked: ${unsynced[*]}"
    printf '       %s\n' "${unsynced[@]}" >&2
    echo "" >&2
    echo "       Their \`.cargo/config.toml\` includes the central \`nros-patch.toml\`" >&2
    echo "       and redirects the message crates at a per-leaf \`generated/\` tree." >&2
    echo "       Both are produced, not committed, so a checkout set up by the book's" >&2
    echo "       install flow alone does not have them yet (issue 0378):" >&2
    echo "" >&2
    echo "           nros sync            # writes nros-patch.toml + the leaf patch tables" >&2
    echo "           just setup all       # if the generated/ message crates are missing too" >&2
    echo "" >&2
    echo "       Every OTHER leaf lock was still checked. Run this on a synced tree" >&2
    echo "       to cover these two as well." >&2
fi

if [ ${#broken[@]} -gt 0 ]; then
    echo "ERROR: ${#broken[@]} leaf crate(s) failed for a reason that is NOT lock drift (see above)." >&2
    echo "       Fix those first — this gate deliberately does not classify them." >&2
    exit 1
fi

# Baseline: one repo-relative directory per line, '#' comments allowed.
mapfile -t baseline < <(grep -vE '^\s*(#|$)' "$BASELINE" 2>/dev/null | sort -u)

# CLAUDE.md: scratch files belong in `$repo/tmp/` (gitignored), not the system
# `/tmp`. A gate is the worst place to ignore that — it runs on shared CI hosts
# and in containers, where a stale `/tmp/.nros-leaf-*` from another user or a
# killed run is readable by the next one, and `$$` only makes collision
# unlikely rather than impossible. `mktemp` under the repo keeps the scratch
# beside the tree it describes and inside the dir the repo already ignores.
_nros_tmp="$(git rev-parse --show-toplevel)/tmp"
mkdir -p "$_nros_tmp"
_drift="$(mktemp "$_nros_tmp/leaf-drift.XXXXXX")"
_base="$(mktemp "$_nros_tmp/leaf-base.XXXXXX")"
trap 'rm -f "$_drift" "$_base"' EXIT

# `printf '%s\n' "${arr[@]}"` on an EMPTY array still emits one blank line, which
# made the summary read "1 known-drifted" with an empty backlog. Guard both.
if [ ${#drifted[@]} -gt 0 ]; then printf '%s\n' "${drifted[@]}"; fi | sort -u > "$_drift"
if [ ${#baseline[@]} -gt 0 ]; then printf '%s\n' "${baseline[@]}"; fi > "$_base"

new="$(comm -23 "$_drift" "$_base")"
fixed="$(comm -13 "$_drift" "$_base")"

fail=0
if [ -n "$new" ]; then
    echo "ERROR: leaf Cargo.lock drift in crate(s) not covered by the baseline:" >&2
    printf '%s\n' "$new" | sed 's/^/       /' >&2
    echo "       Their manifest changed and the lock was not regenerated, so the lock" >&2
    echo "       pins nothing and the crate resolves fresh on every build (issue 0359)." >&2
    # The remedy must not tell the reader to break the lockfile rule. A bare
    # `cargo generate-lockfile` RUNS here (the `--locked` shim does not block
    # it) and re-resolves EVERY package to latest-compatible — measured on
    # `qos-override-pubsub`: "Locking 104 packages to latest Rust 1.97.1
    # compatible versions" where the actual drift was one removed entry. That
    # is how 26 leaf locks once moved 5388 lines as a "cleanup". Same wording
    # as `check-submodule-pinned-locks.py` on purpose: one spelling of the
    # sanctioned move, not a second.
    echo "       Update it the sanctioned way — never a bare \`cargo generate-lockfile\`:" >&2
    echo "           just lock-update \"\" \"\" <leaf-dir>" >&2
    echo "       then REVIEW the diff — if it adds registry packages, that is a" >&2
    echo "       dependency change, not a refresh." >&2
    fail=1
fi
if [ -n "$fixed" ]; then
    echo "ERROR: baselined leaf crate(s) no longer drift — remove them from $BASELINE:" >&2
    printf '%s\n' "$fixed" | sed 's/^/       /' >&2
    echo "       The baseline is a shrinking backlog, not a permanent exemption." >&2
    fail=1
fi
[ "$fail" -eq 0 ] || exit 1

if [ ${#msg_drift[@]} -gt 0 ]; then
    echo "note: ${#msg_drift[@]} leaf lock(s) differ ONLY in generated msg-crate versions —"
    echo "      that is this host's ament environment, not a dependency change, and"
    echo "      regenerating would just bake a different host's versions in (issue 0378)."
    printf '        %s\n' "${msg_drift[@]}"
fi

if [ ${#drifted[@]} -eq 0 ]; then
    echo "leaf lockfiles OK — every tracked leaf lock satisfies its manifest."
else
    echo "leaf lockfiles OK — ${#drifted[@]} known-drifted (issue 0359 backlog), no new drift."
fi
