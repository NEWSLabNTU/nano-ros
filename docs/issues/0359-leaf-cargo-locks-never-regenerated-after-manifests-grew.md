---
id: 0359
title: "24 leaf Cargo.locks were never regenerated after their manifests grew; 18 would pull in new registry crates at today's resolution"
status: resolved
severity: P2
area: build
created: 2026-07-31
refs:
  - issue 0182
  - RFC-0061
---

## Summary

Of 49 tracked leaf `Cargo.lock` files (excluding `third-party/` and
`packages/cli/`), **24 cannot satisfy their own manifest without changing** —
`cargo metadata --locked` refuses them. They are not merely out of date in the
cosmetic sense: their manifests gained dependencies and the locks were never
regenerated.

Found while running tier 1 for phase-318 acceptance, which regenerated three of
them as a side effect. Those three are fixed (`e2cc5d91d`); this issue is the rest.

## The inspection

Regenerating each drifted lock and classifying the diff — then restoring — gives:

| classification | count | meaning |
| --- | --- | --- |
| **PATH-ONLY** | 6 | only local path-dep versions / dep lists move (no `source =` line). Metadata catch-up. Safe. |
| **REGISTRY** | 18 | a crates.io package enters or moves. A real dependency change on an embedded target. |
| no change | 1 | flagged by the probe, regenerates identically |

The REGISTRY cases are not small:

| leaf | changed lines |
| --- | --- |
| `packages/boards/nros-board-nuttx-qemu-arm` | 704 |
| `packages/boards/nros-board-threadx-qemu-riscv64` | 416 |
| `packages/boards/nros-board-fvp-aemv8r-smp` | 410 |
| `packages/boards/nros-board-s32z270dc2-r52` | 410 |
| `packages/boards/nros-board-threadx-linux` | 404 |
| `packages/drivers/serial/cmsdk-uart`, `packages/drivers/serial/stm32f4-usart` | 171 each |
| `packages/rmw/zenoh/zpico-serial` | 169 |

## What it actually is (this is the load-bearing part)

`nros-board-nuttx-qemu-arm` regenerates with **86 packages added and 0 removed.**

So this is not version churn on a stable graph. The manifests grew dependencies —
the graph genuinely got bigger — and the locks never caught up. Regenerating today
therefore does not "restore" anything: it pins 86 registry crates at whatever
resolves at the moment someone runs it.

That is why a bulk `cargo update`-style refresh is the wrong fix, and why it was
deliberately not done alongside `e2cc5d91d`.

## Why it matters

- **The locks are not currently pinning what gets built.** Any build of these
  leaves resolves fresh, so two developers (or a developer and CI) can compile the
  same commit against different dependency versions. That is the class issue #182
  exists about, one layer out: a committed artifact that looks authoritative and
  is not consulted.
- **It is invisible.** Nothing runs `--locked` over these leaves, so the drift
  grows silently with every manifest edit and surfaces only when someone happens
  to build one — which is exactly how it surfaced here.
- **Embedded targets are where an unintended dependency move hurts most**, and 12
  of the 18 REGISTRY cases are board or driver crates.

## Reproduce

```sh
git ls-files "*Cargo.lock" | grep -v "^third-party/" | grep -v "^packages/cli/" \
  | while read -r l; do d=$(dirname "$l"); \
      cargo metadata --locked --format-version 1 --manifest-path "$d/Cargo.toml" \
        2>&1 >/dev/null | grep -q "cannot update the lock file" && echo "$d"; done
```

Caveat: ~10 further leaves fail this probe for unrelated reasons (they need
generated message crates, or carry their own workspace config), so **24 is a lower
bound** — the three fixed in `e2cc5d91d` were themselves unjudgeable by it.

## Fix sketch

Two separable pieces, and the order matters:

1. **Land the 6 PATH-ONLY refreshes now.** Same shape as `e2cc5d91d`: no registry
   crate moves, so no rebuild is implied.
2. **Treat the 18 REGISTRY ones as a dependency change, not a lockfile chore.**
   Regenerate, review what the 86-package additions actually are, and build+test
   the affected boards. Splitting per board keeps each diff reviewable — a single
   commit touching five board locks is not something anyone can check.

Then **gate it**, or it silently returns: a `check-fast`-cheap sweep asserting
every tracked leaf lock satisfies its manifest under `--locked`. The gate must
handle the ~10 unjudgeable leaves explicitly (skip with a recorded reason) rather
than silently passing them — a gate narrower than its rule is issue 0196's class.

## Not in scope

Whether these leaves should have individual lockfiles at all. Several are
`nros sync`-managed (RFC-0048 W9) and the consolidation question belongs with
phase-321/322's package reorganisation, not here.

## Gate landed (2026-07-31)

`scripts/check-leaf-lockfiles.sh` (`just check-leaf-lockfiles`, wired into
`check-fast`) runs `cargo metadata --locked --offline` over every tracked leaf
lock and fails on CHANGE against
`scripts/leaf-lockfile-drift-baseline.txt` — in both directions:

- a leaf drifts that is not baselined → new drift, which is what this stops;
- a baselined leaf stops drifting → the line must be deleted, so the backlog can
  only shrink and cannot become a permanent exemption list.

Mutation-tested both ways. Network-free and ~4s, so it sits in `check-fast`.

**Deliberately does NOT fix the 26 drifted leaves.** That is the pinning
decision this issue is about, and it is untouched.

Three findings while building it:

1. **The count is 24 drifted, not 27.** An earlier sweep that treated any
   non-zero exit as drift over-counted. The gate matches cargo's specific
   "cannot update the lock file … because --locked was passed" message, so a
   broken manifest or a missing vendored dep is reported as its own class rather
   than mis-taught as lock drift.
2. **Two pre-existing workspace holes**, same class as phase-320 W1.b:
   `packages/reference/stm32f4-porting/{polling,rtic}` were in neither `members`
   nor `exclude`, so cargo answers any command inside them with "current package
   believes it's in a workspace when it's not". Now excluded.
3. **A pre-existing broken path dep in both templates**:
   `nros-smoltcp = { path = "../../../../drivers/nros-smoltcp" }` — four `../`
   where three are needed, so it pointed at `<repo>/drivers/`. Every other dep in
   those files uses three. Nothing ever built these templates
   (`packages/reference/README.md` says so), which is exactly why it survived.
   Fixed; they are now ordinary backlog entries.

`tests/simple-workspace` is skipped with a reason rather than baselined: it
ships no `.cargo/config.toml`, so its registry-style `nros-core` dep only
resolves after `nros sync` writes the patch table. It fails identically online
and offline, so `--locked` says nothing about its lock.

## Resolved (2026-07-31) — all 26 pinned

Every baselined leaf was regenerated with `cargo generate-lockfile` and now
satisfies `cargo metadata --locked`. The baseline file is empty; the gate
reports "every tracked leaf lock satisfies its manifest".

The graph really did grow, as predicted — and in both directions:

| leaf | packages before -> after |
| --- | --- |
| `nros-board-nuttx-qemu-arm` | 23 -> 109 (+86) |
| `nros-board-threadx-qemu-riscv64` | 62 -> 111 |
| `nros-board-threadx-linux` | 62 -> 109 |
| `nros-board-fvp-aemv8r-smp`, `nros-board-s32z270dc2-r52` | 24 -> 72 each |
| `nros-board-stm32f4` | 175 -> **159** |
| `nros-board-mps2-an385` | 145 -> **133** |

Three leaves SHRANK: their locks carried packages the manifest no longer pulls.
Eight changed no registry lines at all (pure path/metadata catch-up).

Verification beyond the gate: `just rust-rtos-link-check` passes, and
`nros-board-threadx-linux` and `nros-board-mps2-an385` — two of the largest
graph changes, on different targets — both `cargo build --locked` clean. So the
newly pinned versions compile, not merely resolve.

### The gate stopped using `--offline`, and pinning is what proved it wrong

The first version ran `cargo metadata --locked --offline` to stay network-free.
Pinning exposed the flaw immediately: eleven leaves reported
`failed to download cortex-m-rt v0.7.6` purely because the newly pinned version
had never been fetched on this machine. Offline conflates "the lock cannot
satisfy its manifest" with "this crate is not in the local cache" — and on a
cold CI cache that is EVERY leaf, so the gate would have been red for a reason
unrelated to lockfiles.

It now resolves normally (~30s). CI downloads crates for every build anyway, so
allowing the fetch costs nothing real, and the check means what it says.

