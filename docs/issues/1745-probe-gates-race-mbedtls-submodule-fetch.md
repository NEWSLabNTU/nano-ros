---
id: 1745
title: "Two parallel probe gates provision the mbedtls submodule at once, and one
  dies on `fatal: shallow file has changed since we read it`"
status: open
type: bug
area: [ci, build, cli]
severity: low
found: 2026-10-07
related: [1553, 1621]
---

## Measured

`gate` push run **37603032522** on `main` (head `a96b43d85`, 2026-10-07T09:47Z),
job **112732997659** `check (fast + PR source gates; …)`, step `just check fast`:

```
===== FAIL (probe-workspace-caps, rc=1, 2401ms) =====
=== B. with no caps, an unbounded member is still refused by name ===
[FAIL] [no caps] probe configure failed
[setup] fetching packages/rmw/zenoh/zpico-sys/mbedtls via nros setup --source mbedtls ...
fatal: Needed a single revision
fatal: Unable to find current revision in submodule path 'packages/rmw/zenoh/zpico-sys/mbedtls'
fatal: shallow file has changed since we read it
Error: provision source mbedtls
   0: git fetch --depth 1 5e146adef63b326b04282252639bebc2730939c6 (packages/rmw/zenoh/zpico-sys/mbedtls, source mbedtls)
check-fast (parallel): 1 of 401 gate(s) FAILED
```

`shallow file has changed since we read it` is git's own report that a second
process rewrote the submodule's `shallow` file during this fetch. The fast line
runs its gates in parallel, and more than one probe gate's configure bootstraps
the zpico sources through `nros setup --source mbedtls`. Two of them fetched into
the same `packages/rmw/zenoh/zpico-sys/mbedtls` at the same moment, and the loser
failed its configure.

## It is a single occurrence

The previous five push `gate` runs on `main` that reached a verdict passed
(08:56, 08:14, 07:16, 06:33, 03:46). The next ones (10:17, 10:23) were still
running when this was filed. One red, from a race.

## What it is NOT

- **Not 1553.** That was the same race shape, but the fetch was Corrosion's
  configure-time clone into a shared FetchContent cache. It was fixed by
  provisioning Corrosion in the image. This fetch is a source submodule
  provisioned by `nros setup --source`, which that fix does not cover.
- **Not 1621.** That is a linked worktree left with a broken gitlink after a
  failed provision. This is a CI checkout, and nothing here says the tree was
  left unusable. The next run's outcome will tell.
- Not a code defect in `probe-workspace-caps`: the gate never reached its
  assertion.

## What would close it

Serialise the submodule provision so two concurrent callers cannot fetch into
one module dir. For example, `nros setup --source` could take a lock on the
module's git dir, or the fast lane could provision zenoh-pico/mbedtls once
before it fans out (as the workflow already deepens the submodules it pins).
Acceptance: two probe gates launched concurrently against an unprovisioned
mbedtls both configure. Measure that locally, the way 1553 was measured.
