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
        gv="$(grep -m1 '^version' "$m/Cargo.toml" 2>/dev/null | cut -d'"' -f2)"
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

drifted=()
broken=()
unsynced=()
msg_drift=()
while read -r lock; do
    dir="$(dirname "$lock")"
    if grep -qE "$SKIP_RE" <<<"$dir"; then
        continue
    fi
    if out="$( cd "$dir" && cargo metadata --locked --format-version 1 2>&1 >/dev/null )"; then
        continue
    fi
    if grep -qE "$DRIFT_RE" <<<"$out"; then
        if detail="$(msg_version_drift_only "$dir")"; then
            msg_drift+=("$dir")
            printf '%s\n' "$detail" >/dev/null
            continue
        fi
        drifted+=("$dir")
    elif grep -qE "$UNSYNCED_RE" <<<"$out"; then
        unsynced+=("$dir")
    else
        broken+=("$dir")
        printf '  %s\n' "$dir" >&2
        printf '%s\n' "$out" | head -3 | sed 's/^/      /' >&2
    fi
done < <(git ls-files '*/Cargo.lock' | grep -v '^third-party/' | grep -v '^packages/cli/')

if [ ${#unsynced[@]} -gt 0 ]; then
    # Is the TREE unsynced, or is this leaf broken on a synced tree? The central
    # `nros-patch.toml` answers it: `nros sync` writes it, `.gitignore` excludes
    # it, so a fresh clone never has one.
    #
    # Issue 0466 — this used to `exit 1` either way, which made the gate
    # unrunnable in the very lane it lives in. `check-fast` is documented
    # BUILDLESS *and* SOURCE-FREE, and pr-checks.yml deliberately does NOT
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
    if [ -f "nros-patch.toml" ]; then
        echo "ERROR: ${#unsynced[@]} leaf crate(s) cannot resolve on a SYNCED tree." >&2
        printf '       %s\n' "${unsynced[@]}" >&2
        echo "" >&2
        echo "       \`nros-patch.toml\` exists, so this is not the setup gap below —" >&2
        echo "       these leaves are genuinely unresolvable. Re-run \`nros sync\`; if" >&2
        echo "       they persist, their patch tables or \`generated/\` trees are wrong." >&2
        exit 1
    fi
    echo "check-leaf-lockfiles: SKIP ${#unsynced[@]} leaf crate(s) — tree not synced" >&2
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

# `printf '%s\n' "${arr[@]}"` on an EMPTY array still emits one blank line, which
# made the summary read "1 known-drifted" with an empty backlog. Guard both.
if [ ${#drifted[@]} -gt 0 ]; then printf '%s\n' "${drifted[@]}"; fi | sort -u > /tmp/.nros-leaf-drift.$$
if [ ${#baseline[@]} -gt 0 ]; then printf '%s\n' "${baseline[@]}"; fi > /tmp/.nros-leaf-base.$$
trap 'rm -f /tmp/.nros-leaf-drift.$$ /tmp/.nros-leaf-base.$$' EXIT

new="$(comm -23 /tmp/.nros-leaf-drift.$$ /tmp/.nros-leaf-base.$$)"
fixed="$(comm -13 /tmp/.nros-leaf-drift.$$ /tmp/.nros-leaf-base.$$)"

fail=0
if [ -n "$new" ]; then
    echo "ERROR: leaf Cargo.lock drift in crate(s) not covered by the baseline:" >&2
    printf '%s\n' "$new" | sed 's/^/       /' >&2
    echo "       Their manifest changed and the lock was not regenerated, so the lock" >&2
    echo "       pins nothing and the crate resolves fresh on every build (issue 0359)." >&2
    echo "       Fix: cd <dir> && cargo generate-lockfile, then REVIEW the diff — if it" >&2
    echo "       adds registry packages, that is a dependency change, not a refresh." >&2
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
