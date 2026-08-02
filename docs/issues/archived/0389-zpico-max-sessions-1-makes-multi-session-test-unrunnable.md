---
id: 389
title: "`ZPICO_MAX_SESSIONS` defaults to 1, so the multi-session work from #348/#376 has no test that ever runs in a default tree"
status: resolved
resolved_in: b1db98829
type: tech-debt
area: testing
related: [issue-0348, issue-0376, issue-0388, phase-328]
---

# The multi-session tests never run

## What happens

`two_sessions_deliver_cross_session_through_router` — the e2e proof that two
zpico sessions on one host deliver to each other through the router — skips in
every default build:

```
panicked at packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs:242:
  [SKIPPED] second session refused — shim built with ZPICO_MAX_SESSIONS=1;
  rebuild with ZPICO_MAX_SESSIONS=2 to exercise multi-session
```

`ZPICO_MAX_SESSIONS` defaults to **1** (`packages/rmw/zenoh/nros-zpico-build/
src/lib.rs:23,56`), and nothing in the repo's own build configuration raises it,
so the skip fires on every host, in every tier, always.

## Why it matters now

Multi-session support is not speculative any more — it is the substance of two
issues closed this cycle:

- **#348 / phase-328** — zpico multi-session, full handle-passing.
- **#376** — the Rust shim's `SERVICE_BUFFERS` / `REPLY_WAKERS` were
  process-global and were session-scoped as the fix. `service.rs:99-112` sizes
  those tables `ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES` and comments that "at
  the default `ZPICO_MAX_SESSIONS == 1`" the layout collapses to the old one.

So the code paths those issues added are, in a default tree, either unreachable
or degenerate — and the one test written to exercise them is skipped. The fixes
are effectively unverified by CI. That is the same shape as the silent-lane class
(#0202 nothing ran the CLI tests, #0319 a backend suite on no lane, #0379 no
clippy on a sub-workspace): the test exists, so it looks covered.

Worse, the skip is currently reported as a FAILURE by `just test-unit` (issue
0388 D2), which means the one signal that this is not being exercised reads as
noise people learn to ignore.

## Direction

1. Build at least one test target with `ZPICO_MAX_SESSIONS=2` so the multi-session
   path is exercised somewhere that runs by default — a dedicated fixture row, or
   a per-test build config, rather than raising the global default (single-session
   targets should keep the smaller static footprint, which is the point of the
   knob).
2. Make that cell a declared coverage row per RFC-0051, so the pairing is visible
   in `matrix::CELLS` / `interop::CELLS` instead of living only in a skip message.
3. Re-check `service.rs`'s session-scoped tables under `ZPICO_MAX_SESSIONS=2`:
   the fix for #376 is exactly what a default build cannot reach.

## Evidence

Ubuntu 22.04 distrobox, checkout `1d192d4f2`, `just test-unit`:
`817 tests run: 816 passed, 2 skipped` plus this one counted as a failure. The
skip text names the remedy, and no in-tree build applies it.

## Resolved (2026-08-02, `b1db98829`)

`just test-zpico-multisession` builds the shim at `ZPICO_MAX_SESSIONS=2` in its
own `CARGO_TARGET_DIR` and runs the cross-session test; it is a dependency of
`just test`, so it runs regularly rather than only in a sweep. The global default
stays 1, so shipped and copy-out builds keep the smaller footprint (direction 1).

The test **passes** — its first execution ever, which is the actual verification
#348 and #376 were missing.

Two false starts worth keeping:

- Putting `[env] ZPICO_MAX_SESSIONS = "2"` in the ROOT `.cargo/config.toml` looks
  like the tidy fix and is wrong: cargo MERGES config files up the directory
  tree, so the in-tree examples inherit it (verified with `cargo config get env`
  from `examples/native/rust/talker`). Every embedded example would have doubled
  its session tables — the exact regression the default guards against.
- The separate target dir is load-bearing, not cosmetic: the variable is a build
  input (`env_usize` emits `rerun-if-env-changed`), so sharing `target/` makes
  the two lanes rebuild the C shim back and forth.

Also note for anyone filtering this test by hand: `zenoh_integration.rs` is
`#![cfg(feature = "platform-posix")]`, so `cargo nextest run -p nros-rmw-zenoh
--test zenoh_integration -E 'test(~two_sessions)'` lists ZERO tests without
`--features platform-posix`. That silence looks exactly like a filter typo.

Verified isolation, both target dirs alive simultaneously:
`target/…` → `ZPICO_MAX_SESSIONS: usize = 1;`,
`target-zpico-multisession/…` → `= 2;`, with `just test-unit` still exit 0 and
the test still reported `[SKIPPED]` in the default tier.

Direction 2 (a declared RFC-0051 coverage row) is NOT done — this is a justfile
lane, not a `matrix::CELLS` entry. Worth doing if the multi-session surface grows
beyond one test.
