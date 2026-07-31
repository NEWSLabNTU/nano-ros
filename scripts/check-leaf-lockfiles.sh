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
# GREW, so `nros-board-nuttx-qemu-arm` alone comes back with 86 packages added
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

# `--offline` on purpose: the check must not touch the network in CI. The drift
# message is byte-identical with and without it (cargo reports the refusal
# before it would fetch), so precision costs nothing.
#
# Match that message specifically rather than treating ANY non-zero exit as
# drift: a missing vendored dependency or a broken manifest must not be
# misreported as lock drift, or the gate teaches people the wrong fix.
DRIFT_RE='cannot update the lock file .* because --locked was passed'

# Leaves that cannot resolve STANDALONE by design, so `--locked` says nothing
# about their lock. `tests/simple-workspace` ships no `.cargo/config.toml`: its
# `nros-core` dep is registry-style and only resolves once `nros sync` writes
# the patch table, so cargo searches crates.io and fails identically online and
# offline. Skipping it is honest; classifying it as drift would be a lie, and
# leaving it as "broken" would make the gate permanently red.
SKIP_RE='^(tests/simple-workspace)$'

drifted=()
broken=()
while read -r lock; do
    dir="$(dirname "$lock")"
    if grep -qE "$SKIP_RE" <<<"$dir"; then
        continue
    fi
    if out="$( cd "$dir" && cargo metadata --locked --offline --format-version 1 2>&1 >/dev/null )"; then
        continue
    fi
    if grep -qE "$DRIFT_RE" <<<"$out"; then
        drifted+=("$dir")
    else
        broken+=("$dir")
        printf '  %s\n' "$dir" >&2
        printf '%s\n' "$out" | head -3 | sed 's/^/      /' >&2
    fi
done < <(git ls-files '*/Cargo.lock' | grep -v '^third-party/' | grep -v '^packages/cli/')

if [ ${#broken[@]} -gt 0 ]; then
    echo "ERROR: ${#broken[@]} leaf crate(s) failed for a reason that is NOT lock drift (see above)." >&2
    echo "       Fix those first — this gate deliberately does not classify them." >&2
    exit 1
fi

# Baseline: one repo-relative directory per line, '#' comments allowed.
mapfile -t baseline < <(grep -vE '^\s*(#|$)' "$BASELINE" 2>/dev/null | sort -u)

printf '%s\n' "${drifted[@]}" | sort -u > /tmp/.nros-leaf-drift.$$
printf '%s\n' "${baseline[@]}" > /tmp/.nros-leaf-base.$$
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

echo "leaf lockfiles OK — $(wc -l < /tmp/.nros-leaf-drift.$$) known-drifted (issue 0359 backlog), no new drift."
