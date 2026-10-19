---
id: 1745
title: "Two parallel probe gates provision the mbedtls submodule at once, and one
  dies on `fatal: shallow file has changed since we read it`"
status: resolved
type: bug
area: [ci, build, cli]
severity: low
found: 2026-10-07
resolved_in: "branch fix/phase-505-w2-provision"
related: [1553, 1621, 1775, phase-505]
---

**Home: [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## Resolution (2026-10-19)

**The race this issue measured was closed two days after it was filed, by
[issue 1775](1775-provision-source-races-parallel-gates.md)**, and the path it
ran on no longer exists. What was left sat one directory over, and is fixed
here.

- **1775's lock.** `provision_source` takes an exclusive `flock` per source
  before its presence checks, so a second provisioner waits and then finds the
  work done. That landed 2026-10-09. The red above is from 2026-10-07, on a
  tree without it. It is the same race (two processes, one submodule, git's own
  lock or shallow file), reported in git's other wording.
- **The submodule is no longer touched.** phase-484 W3c moved `mbedtls` and
  `zenoh-pico` to `location = "store"`. A probe gate's `nros setup --source
  mbedtls` now materialises `$NROS_HOME/sources/mbedtls/<version>+<sha8>/`
  with `git archive` from a bare mirror. It never fetches into
  `packages/rmw/zenoh/zpico-sys/mbedtls`.
- **The gap that remained: the store mirror.** 1775's lock for a store source
  sits beside the pin's `dest`, so there is one lock per PIN. The bare mirror
  `$NROS_HOME/fetch/git/<name>.git` is shared by EVERY pin of a source, and the
  store is shared between checkouts. So two worktrees at different pins could
  still fetch `--depth 1` into one shallow repository at once, which is this
  issue's `shallow file has changed`. zenoh-pico has 38 pins in this host's
  store. `object_repo_for` now takes a second lock,
  `fetch/git/.<name>.git.nros-fetch.lock`, around the mirror's init and fetch.
  The per-pin lock is always taken first and the mirror lock second, so the two
  cannot deadlock. A mirror dir with no `HEAD` (a failed `init`) used to count
  as present, and every later run fetched into it. Now it is re-initialised,
  and a failed init removes the directory it made.

### Measured

The acceptance, run locally with this branch's CLI. A scratch `NROS_HOME`
mirrors `~/.nros` by symlink except for mbedtls's tree and bare mirror, so
mbedtls is unprovisioned and everything else is real. `probe-workspace-caps`
and `probe-shared-types` were launched concurrently:

| round | probe-workspace-caps | probe-shared-types | mbedtls materialised |
| --- | --- | --- | --- |
| 1 | rc=0, 5/5 checks | rc=0, 17/17 checks | yes, during the run |
| 2 | rc=0 | rc=0 | yes |
| 3 | rc=0 | rc=0 | yes |

At the provisioning level, against an EMPTY store: 4 concurrent `nros setup
--source mbedtls` passed 4/4 in each of 4 rounds, and 4 concurrent
`zenoh-pico` passed 4/4.

Not reproduced: the cross-pin mirror race. It needs two checkouts at different
pins fetching at the same instant. The lock uses the same `flock` mechanism
that `two_provisioners_of_one_source_wait_for_each_other` tests for 1775.
