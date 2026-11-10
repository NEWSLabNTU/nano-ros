---
id: 2424
title: "A `< <(producer)` redirect drops the producer's exit status, so a crashing `fixtures-manifest.py` reads as an empty row list"
status: resolved
type: bug
severity: medium
area: [build, gates, ci]
related: [1249, 1466, 2363]
found: 2026-11-10
resolved: 2026-11-10
resolved_in: "gate `check-process-substitution-status` (a ratchet) + `wait $!` on the 9 `python3` producer sites"
---

> **Resolved (2026-11-10).** `just check process-substitution-status` refuses a
> new unchecked `< <(...)` site. A per-file ratchet holds the 165 sites still
> unchecked, and every `python3` producer (the shape that hid this one) now
> checks its status with `wait $!`.

## What happened

#3449 (`2865cc77c5`) made `fixtures-manifest.py west-overrides` raise
`UnboundLocalError`. `scripts/build/zephyr-fixture-leaves.sh` reads it as

```bash
while IFS=$'\t' read -r ov_id ov_kv; do
    ...
done < <(python3 "$nros_root/scripts/build/fixtures-manifest.py" west-overrides)
```

so the loop got an empty stream and every Zephyr row built with NO west
overrides, silently. The script runs under `set -euo pipefail`, and neither
option sees the producer: bash runs a process substitution asynchronously, and
the redirect's status is the consumer's. It stayed hidden until #3474
(issue 2363) renamed the local and made that runner fail loudly. Found by the
241 worker.

This is the third shape of one class. Issue 1249 (a bare `var="$(cmd)"` whose
status is inspected) and issue 1466 (a `$(...)` in argument position) each have
a gate, and neither can see a `< <(...)`.

## Measured

187 `< <(` sites in tracked shell (`*.sh`, `*.just`, `justfile`, `.githooks/*`,
workflows; the same file set as `check-set-e-bare-assignment`). With `printf` /
`echo` producers exempt, 174 were unchecked. Their producer heads were mostly
`git` (43), `grep`, `find`, local shell functions, and 9 `python3` scripts.

`wait $!` returns a process substitution's status on bash ≥ 4.4 (measured on
bash 5.1.16, for both `mapfile` and a `while read` loop):

```bash
mapfile -t b < <(echo y; exit 4); wait $! || echo "wait rc=$?"   # wait rc=4
```

## Fix

- **Gate** `scripts/check-process-substitution-status.py` (`just check
  process-substitution-status`, fast line). A site counts as checked when
  `wait $!` is the next statement (same line after `;`, or the next non-blank,
  non-comment line). A `printf`/`echo` producer is exempt. The gate
  self-tests ten shapes first, including a multi-line producer with a quoted
  paren and `waiting=1`, which does not count as `wait`.
- **Ratchet** `.config/process-substitution-status-baseline.txt`, per-file
  counts that may only fall. A count above the baseline fails, and so does one
  below it (`--update` lowers the baseline; it refuses to raise it). Rewriting
  all 174 sites at once would be the mass rewrite issue 1249's gate declined
  to do.
- **The 9 `python3` producers** now check their status with `wait $!`: all
  `fixtures-manifest.py` readers in `zephyr-fixture-leaves.sh` (2),
  `check-fixtures-stale.sh` (2), `nuttx-libc-pin-guard.sh` (2),
  `compile-check-fixtures.sh`, `just/zephyr-ci.just`, and `file_kinds.py` in
  `check-build-profile-literals.sh`. Each refuses with a message naming the
  producer and this issue.

Mutation: appending an unchecked `done < <(python3 nope.py)` to a baselined
file turns the gate red and names the site.

Sweep: `git grep -nE '< <\(' -- scripts just justfile .githooks .github/workflows`
