---
id: 466
title: "Tier 1 has an unstated, ORDERED setup contract — eight consecutive
  blockers, one of them a test"
status: open
type: bug
area: build
related: [issue-0422, issue-0430, issue-0431, issue-0451, rfc-0061, phase-318]
---

## Symptom

`just ci` is the instruction every task ends with (CLAUDE.md, RFC-0061). Over one
session it was attempted repeatedly on a correctly-cloned, ROS-provisioned tree
and stopped **eight** times. Each stop was real, each was invisible until the
previous one cleared, and only ONE was a test failure:

| # | where it stopped | what it actually was |
| --- | --- | --- |
| 1 | `check-examples`, ~3x slower | no GNU `parallel`; the lane DEGRADES to serial |
| 2 | `_require-build-sources` | in-tree `nros` stale after a tree refresh |
| 3 | `nros_box_publish` | read `$CARGO_TARGET_DIR`, which is correctly UNSET |
| 4 | `check-leaf-lockfiles` | two NuttX leaf locks their manifests outgrew |
| 5 | `rust-rtos-link-check` | NuttX static link args, fixed upstream meanwhile |
| 6 | `check-build-profile-literals` | a waiver detached by an inserted comment |
| 7 | `check-no-tracked-file-find` | a checker that walked the tree it scanned |
| 8 | `check` | `nros-board-mps2-an385` did not COMPILE on main |

Nothing here is exotic. The tree was clean, the toolchains installed, the SDKs
provisioned. The cost is not any single item — it is that the list is serial and
undiscoverable: fixing one reveals the next, ~40 minutes later.

## Two distinct problems, and they compound

### A. The setup contract is real, ordered, and written nowhere (1, 2, 3, 5)

There is a sequence a tree must satisfy before `just ci` means anything:

```
nros setup --system          # system packages from [system.*]
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
nros_box_publish             # or the host equivalent
just setup-launch-resolve
just build-test-fixtures lane=<the lane you will test>
just ci
```

Every step is documented SOMEWHERE. The sequence is documented nowhere, and the
order is load-bearing: the CLI must be rebuilt before fixtures, because fixtures
key on the CLI's stamp; fixtures must be built for the lane, because
`_require-fixtures` checks coverage per lane (#393).

Worse, the steps re-arm each other. **Any** tree refresh — pull, rebase, stash,
rsync — restages the CLI's source stamp AND every fixture mtime (CLAUDE.md's
"fixture mtime treadmill"). So the contract is not once-per-clone, it is
once-per-refresh, and a session that syncs mid-run silently invalidates work it
already did.

The failure mode is what makes this a bug rather than a docs gap: **most of
these degrade instead of failing.** No `parallel` prints one line and walks 99
example leaves serially — on a 4-core box that reads as a hung tier, not as a
missing package. `just doctor` had been reporting `[MISSING] gnu-parallel` with
the exact apt line the entire time; nothing in the box bootstrap said to run it.
Compare #0451 (embedded SDK env), which is the same shape one layer down: a
required set, surfaced one variable at a time, each looking like a code fault.

### B. Tier 1 is where the source gates live, and it is too expensive to reach (4, 6, 7, 8)

Four of the eight were source-level reds ALREADY ON MAIN — including one crate
that simply did not compile (`mod log_bridge;` committed without its file). They
sat there because the gates that catch them run only in `just ci`, and `just ci`
was unreachable behind problem A.

The consequence is measurable, not hypothetical: **two of the four were fixed
CONCURRENTLY by a different session while this one was fixing them** — the px4
profile-literal waiver and the `git grep` scan, same files, same hour, both
patches landing as duplicates on rebase. That is the signature of a red backlog
that many sessions rediscover independently in different orders.

A ~40-minute serial gauntlet is also a strong incentive to skip the tier, which
is precisely the dynamic RFC-0061 created the ladder to avoid: "the old single
`just ci` WAS tier 3 — an instruction nobody could afford per task, so it got
followed selectively, which is worse than a smaller instruction followed
honestly." Tier 1 is now re-acquiring that property, not through its own runtime
but through everything required before it starts.

## Why the existing pieces do not cover it

- `just doctor` KNOWS about (1). It is not on any path a person walks before a
  tier run, and the box bootstrap never mentions it. Partially addressed: the
  bootstrap and the non-Ubuntu guide now run `nros setup --system` then
  `just doctor`, with the reason inline.
- The stale-CLI guard (#0430) fires correctly for (2) — and #0430 is resolved.
  What is missing is that nothing tells you a tree REFRESH re-arms it.
- `_require-fixtures` (#393) covers lane coverage but not the ordering above it.
- #0451 is the same class for embedded env vars, scoped to direct builds.

## What would fix it

Ordered by how much each buys, not by effort:

1. **A single precondition gate at the head of `ci`** that checks the whole
   contract and prints every unmet item at once with its remedy — the way
   `nros setup --system --check` already does for packages. One failure listing
   four things beats four failures listing one thing each.
2. **Make the degrading probes loud when they matter.** A lane that silently
   drops to serial should say so as a WARNING with the remedy (done for
   `check-examples`); better, `ci` should refuse a degraded lane unless asked.
3. ~~A cheap `just ci --gates-only`~~ — **CORRECTION: this lane already exists,
   and was itself blocked.** `check-fast` is documented as exactly that
   ("BUILDLESS, SOURCE-FREE gates only … needs neither the nros CLI nor any
   provisioned source … ~1 min … the per-push CI gate"). Its FIRST dependency
   was `check-cli-fresh`, so on a tree whose only sin was a `git pull`:

   ```
   just check-fast   ->  failed in 0.77s, having checked NOTHING
   ```

   The early placement came from #0363 — front-run `check-dep-chain`, which
   surfaced a stale CLI minutes in as nine cells failing with a cargo resolution
   error. Correct intent, wrong lane: `check-dep-chain` is a check-BUILD gate,
   and no fast-tier gate execs the CLI at all.

   **FIXED (ccd63f474):** moved to the head of `check-build`, which still
   front-runs `check-dep-chain` and gives `check-fast` its contract back. Same
   tree, same stale CLI: 0.77s -> 73s, 19+ gates reporting, including all three
   fast-tier gates that caught this issue's own reds — `check-leaf-lockfiles`,
   `check-build-profile-literals`, `check-staleness-probe-exemptions`.

   So (B) was never a missing-lane problem. The lane existed, ran per-push in
   `check.yml`, and had been disabled for anyone whose CLI was out of date —
   which is anyone who pulled.

   Two things this did NOT fix, both left deliberately:
   - `check-fast` still is not purely buildless: `check-test-targets` compiles
     the test targets, added on purpose after two incidents where a struct field
     broke every test initializer while `check-fast` stayed green. Re-scoping
     what the per-push lane covers is a coverage decision, not a cleanup.
   - On an Arch host that compiling gate now fails for an unrelated reason:
     `pyo3-ffi 0.24.2` caps at Python 3.13 and Arch ships 3.14, so anything
     building `packages/cli` dies with "the configured Python interpreter
     version (3.14) is newer than PyO3's maximum supported version (3.13)".
     Ubuntu 22.04 (Python 3.10) is unaffected, which is why every box run got
     past it. Probably its own issue.
4. Fold "any refresh re-arms the CLI stamp and every fixture" into the pitfall
   index next to the mtime treadmill, which currently names fixtures only.

## Evidence

Session of 2026-08-06/07, in the ROS distrobox mirror and reproduced on the host
where noted. Fixes that came out of it, all landed:

```
f9bd64890  the log_bridge module ab40ab25e declared but never staged
b573b3fe2  nros_box_publish looked for the CLI under an unset variable
32d11ded5  regenerate the two NuttX FFI leaf locks
d602d959c  box-sync writes the box-owned marker BEFORE the transfer
8abd20cf1  logging_smoke lane token (#422)
```

Plus two duplicate-fix collisions described in (B), and the provisioning/doc
changes in `ros2-distrobox-setup.sh` + `docs/development/ros2-on-non-ubuntu.md`.

The one genuine test failure across all eight attempts is #0422's triage index,
which is being worked separately — 10 real failures, independently reproduced on
a second tree.
