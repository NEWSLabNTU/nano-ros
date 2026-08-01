#!/usr/bin/env bash
#
# Issue 0359 / 0378 — build steps must RESPECT the lockfile, not rewrite it.
#
# THE PROBLEM THIS EXISTS FOR
#
# A lockfile is a promise that someone else's build resolves what yours did.
# `cargo build` breaks that promise silently: when a manifest no longer agrees
# with the lock, it UPDATES THE LOCK as a side effect and carries on. No
# warning, no diff in the output, just a modified tracked file.
#
# That is the actual root cause of issue 0359, which was filed as "leaf locks
# drifted". They did not drift on their own — builds rewrote them, and nothing
# ever asserted otherwise, because at the time this gate was written NONE of
# the 76 cargo build/test/run invocations in `justfile` + `scripts/` passed
# `--locked`. Drift was not just undetected; it was manufactured.
#
# `--locked` inverts it: a manifest/lock mismatch becomes a hard error naming
# the crate, and the only way to change a lock is to mean it —
# `just lock-update`.
#
# WHY A BASELINE
#
# Flipping all 76 call sites in one change is a large mechanical edit whose
# blast radius is every build lane, and it cannot be honestly verified without
# a full sweep. So the existing sites are frozen and NEW ones must be locked:
# the count can only go down, and the file says exactly which are left. A gate
# that cannot pass gets bypassed, and a bypassed gate is worth less than none.

set -euo pipefail
cd "$(dirname "$0")/.."

BASELINE="scripts/cargo-locked-baseline.txt"

# Invocations that BUILD or RESOLVE, so a lock mismatch matters. `cargo fmt`,
# `cargo clippy --version`, `cargo search` and friends never touch a lock.
CARGO_RE='cargo (\+[a-zA-Z0-9._-]+ )?(build|test|run|rustc|nextest run|metadata|tree|fetch)\b'

# `lock-update` is the sanctioned mutator; it exists precisely to write locks.
SKIP_RECIPE='lock-update'

current="$(
    grep -rnE "$CARGO_RE" justfile scripts/*.sh scripts/build/*.sh scripts/ci/*.sh 2>/dev/null |
        grep -v -- '--locked' |
        grep -v -- '--frozen' |
        grep -vE "$SKIP_RECIPE" |
        # Prose: comment lines and doc text mentioning a command, not running it.
        grep -vE '^[^:]+:[0-9]+:[[:space:]]*#' |
        awk -F: '{print $1":"$2}' |
        sort -u
)"

mapfile -t baseline < <(grep -vE '^\s*(#|$)' "$BASELINE" 2>/dev/null | sort -u)

tmp_cur="$(mktemp)"
tmp_base="$(mktemp)"
trap 'rm -f "$tmp_cur" "$tmp_base"' EXIT
printf '%s\n' "$current" | grep -v '^$' | sort -u >"$tmp_cur"
if [ ${#baseline[@]} -gt 0 ]; then printf '%s\n' "${baseline[@]}"; fi >"$tmp_base"

# Compare by FILE only, not file:line — an unrelated edit above a call site
# shifts its line number and would otherwise read as a brand-new violation.
cut -d: -f1 <"$tmp_cur" | sort | uniq -c | sed 's/^ *//' >"$tmp_cur.by_file"
cut -d: -f1 <"$tmp_base" | sort | uniq -c | sed 's/^ *//' >"$tmp_base.by_file"

status=0
while read -r count file; do
    base_count="$(awk -v f="$file" '$2==f {print $1}' "$tmp_base.by_file")"
    base_count="${base_count:-0}"
    if [ "$count" -gt "$base_count" ]; then
        status=1
        echo "[FAIL] $file: $count unlocked cargo invocation(s), baseline allows $base_count" >&2
        grep -nE "$CARGO_RE" "$file" | grep -v -- '--locked' | grep -vE '^[0-9]+:[[:space:]]*#' |
            sed 's/^/       /' >&2 || true
    fi
done <"$tmp_cur.by_file"

if [ "$status" -ne 0 ]; then
    echo "" >&2
    echo "  A build step that omits --locked REWRITES Cargo.lock on a manifest" >&2
    echo "  mismatch instead of failing (issue 0359). Add --locked; if the lock" >&2
    echo "  genuinely needs to change, change it deliberately:" >&2
    echo "      just lock-update [crate] [version] [dir]" >&2
    exit 1
fi

remaining="$(wc -l <"$tmp_cur" | tr -d ' ')"
echo "cargo --locked OK — $remaining baselined unlocked invocation(s), none new."
