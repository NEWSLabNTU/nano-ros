---
id: 328
title: "Test-harness gaps: ~30 fixture resolvers still existence-only (museum-binary trap #222 not propagated), and 24 #[ignore] tests are permanently unreachable"
status: resolved
type: bug
severity: medium
area: testing
related: [issue-0222, issue-0196, issue-0326]
---

> **Shared pattern with #326 — "the fix landed at the site, not the class."**
> #222 fixed 4 of ~34 identical fixture resolvers; #282 fixed 1 of 6 identical
> cmake guards (→ #326). Same failure mode, different subsystem: a grep-able
> class gets fixed where the symptom appeared, so the remaining siblings stay
> armed and the next incident looks new. Whoever picks up either issue should
> sweep the whole class and land ONE shared helper — see the "Fix the CLASS"
> practice in `CLAUDE.md`, and the audit note in
> `docs/development/audit-findings-2026-07-28.md`.

## Finding (audit 2026-07-28, P2)

Two independent holes in the harness, both of the same family: a check that
exists but does not cover what it should.

### 1. The freshness probe was never propagated out of the RTOS resolvers

Issue #222 swapped the four RTOS fixture resolvers to
`require_prebuilt_binary_fresh*` — verified still in place
(`freertos.rs:85,:162`, `nuttx.rs:125,:205,:244`, `threadx_linux.rs:126,:169`,
`threadx_riscv64.rs:78,:156`).

But **~30 resolvers in `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`
still use existence-only `require_prebuilt_binary`** on cargo-built artifacts that
*do* have a sibling `.d` dep-info file, so the staleness check is available and
simply not used. A museum binary passes every sweep — the issue-0148/0164/0196
trap, just relocated.

Highest-value sites:

| site | covers |
| --- | --- |
| `mod.rs:2380` (`require_shared_fixture_binary`) | all qemu-arm-baremetal shared fixtures |
| `mod.rs:3040` | the freertos rust example path |
| `mod.rs:4195` (`build_zenoh_stress_test_large_buf`) | stress fixture |
| `mod.rs:4434` | the ESP32 elf resolver |
| `zephyr.rs:899`, `:961` | zephyr fixtures |

Fix: switch the cargo-path resolvers to `require_prebuilt_binary_fresh`.

### 2. All 24 `#[ignore]` tests are unreachable

24 `#[ignore]` attributes exist across `packages/`, and **nothing anywhere passes
`--ignored` / `run-ignored`** — not `just/`, not `justfile`, not
`.github/workflows/`, not `.config/nextest.toml`. They are dead code that reads
like coverage.

Distribution: `rosidl-codegen/tests` 8, `zpico/nros-rmw-zenoh` 6,
`nros-tests/tests` 4, `nros-rmw-cyclonedds` 2,
`rosidl-codegen/src/generator` 2, `cargo-nano-ros/src` 1, `nros-sizes-build` 1.

The 8 in `rosidl-codegen/tests` matter most: `heap_compile_check.rs` and its
siblings are the **only** gate on heap/borrowed storage-mode codegen, so that
feature currently has zero executing coverage.

Easiest win: the 5 `zenoh_integration.rs` tests are ignored with
`#[ignore = "requires zenohd router on tcp/127.0.0.1:7447"]`, but
`ZenohRouter::start_unique` makes that precondition self-provisioning — they can
simply be un-ignored.

Fix: add a `just` recipe (or a nextest profile with
`default-filter = "all()"` + `run-ignored`) for the heavy-but-valuable set;
delete or convert the rest. An ignored test with no lane that runs it should fail
review the same way a `#[allow(dead_code)]` without a reason does.

## Why grouped

Both are "the harness has the mechanism and doesn't apply it", both live in
`packages/testing`, and both are the kind of gap that makes a green sweep
misleading — the audit checklist's E1/E4 pair.

## Resolved (2026-07-28)

### Part 1 — the freshness probe, swept across the class

**30 call sites** moved from existence-only `require_prebuilt_binary` to
`require_prebuilt_binary_fresh`, leaving only the three wrapper bodies that
intentionally call the base.

The swap is safe everywhere, which is what made a blanket sweep the right move
rather than a site-by-site audit: `dep_info_newer_source` reads the sibling
`.d` through `fs::read_to_string(...).ok()?`, so a fixture with no dep-info
degrades to exactly the old behaviour. With dep-info, the check engages. There
is no artifact for which the fresh variant is worse.

Two zephyr sites got `require_prebuilt_binary_fresh_zephyr` instead of the
generic one. That distinction matters: a `zephyr.exe` has no `zephyr.exe.d`, so
the generic variant would silently no-op — the zephyr-aware one compares the
staticlib's `.d` against the linked image, which is where the real drift lives.

**Verified engaged, not just compiled:** touching
`packages/core/nros-node/src/lib.rs` now makes `native_entry_boot` hard-fail
`Test fixture is STALE — a source is newer than the built binary … newer:
…/action_core.rs`. That resolver was existence-only before this change and
would have run the museum binary.

### Part 2 — the ignored tests are reachable, and mostly PASS

Added `just test-ignored [package]`. Nothing previously passed `--run-ignored`
anywhere, so 24 tests were dead code that read like coverage.

**The recipe has to span two workspaces**, and that turned out to be the whole
point: 16 of the 24 live in `packages/cli`, which the root workspace cannot see
at all. A root-only recipe would have reported success while running none of
the ones that matter most.

Running it produced the finding the issue could not have known:

| set | result |
| --- | --- |
| `packages/cli` (incl. `rosidl-codegen` storage-mode gates) | **13 run, 13 passed** |
| root workspace, no router | 7 run, 2 passed, 5 failed |
| root workspace, router on `tcp/127.0.0.1:7447` | 7 run, **6 passed**, 1 failed |

So the heap/borrowed storage-mode codegen the issue calls "zero executing
coverage" was in fact **fully working** — 12 passing tests nobody ran. That is a
better outcome than the issue assumed and a worse indictment of the lane gap.

### The "easiest win" IS available after all — corrected twice

My first pass concluded the 5 `zenoh_integration.rs` tests could not be
un-ignored because `ZenohRouter` lives in `nros-tests`, which is not a
dev-dependency of `nros-rmw-zenoh` and (I assumed) could not become one, since
`nros-tests` depends on `nros-rmw-zenoh`.

**That assumption was wrong.** Cargo permits dev-dependency cycles: a crate's
dev-deps may depend back on it. Verified by adding the dev-dep and building —
`cargo metadata` resolves and the test target compiles.

So the tests were rewritten to self-provision: each starts its own
`ZenohRouter::start_unique()` on an ephemeral port, which reaps orphans and
shuts down on `Drop`. The hardcoded `tcp/127.0.0.1:7447` const is gone.

An ephemeral port is also the *correct* mechanism rather than a slot from
`nros_tests::alloc`. That allocator serves baked-isolation cells, whose images
compile a locator in; its own module docs say native host tests should take
runtime-ephemeral ports, which are parallel-safe by construction.

Result: **12 of 13 pass with no external setup**, where previously 5 never ran
at all.

### Superseded: the original reachability claim

(Kept for the record.) The intermediate finding — that with a router on 7447
four of the five pass — was correct and is what made the dev-dep experiment
worth running.

### Left open deliberately

- `test_pubsub_separate_sessions` **fails even with its precondition met** —
  now filed as **issue 0347**. Two independent client sessions on the same
  zenohd never exchange data, while same-session pub/sub passes against that
  same router. Ruled out as a timing race: it also fails with the publisher
  republishing every 100 ms for ten seconds. It is the only remaining
  `#[ignore]` in the file, and its reason now names that issue instead of the
  router precondition it no longer has.
- `just test-ignored` is not wired into `just ci`. Several of these need
  external infrastructure by design, which is why they are ignored; the fix for
  this issue is that they are runnable and their state is knowable, not that
  they gate every push.
