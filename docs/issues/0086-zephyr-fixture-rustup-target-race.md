---
id: 86
title: Zephyr fixture build races on rustup component downloads (parallel west builds)
status: open
type: bug
area: zephyr
related: [phase-258]
---

## Symptom (2026-06-19)

`just zephyr build-fixtures` (the parallel west-fixture leaf) fails mid-build with
rustup component-download errors — even for targets that are **already installed**:

```
error: component download failed for rust-std-x86_64-unknown-none: could not rename
  'downloaded' file from '~/.rustup/downloads/<hash>.partial' to '~/.rustup/downloads/<hash>':
  No such file or directory (os error 2)
FATAL ERROR: command exited with status 1: cmake --build .../build-rs-talker-zenoh
```

Seen for `rust-std-x86_64-unknown-none`, `rust-std-armv7a-none-eabi`,
`rust-std-armv7a-none-eabihf` while building the 4.4 native_sim zenoh fixture set
(talker / listener / service-client / service-server) on a host with network.

## Root cause

The fixture scheduler fans out N concurrent `west build` → `cargo build` →
`rustc`/`rustup` invocations. Each cargo build triggers rustup's component
check/ensure, and the concurrent invocations **collide on the shared
`~/.rustup/downloads/<hash>.partial`** staging file: one process renames/removes it
out from under another → "could not rename … No such file". A missing target is the
trigger (first fetch), but the race then corrupts even already-installed targets'
re-verification.

Confirmed it is a race, not a hard network block: adding the missing target
**serially** (`rustup target add armv7a-none-eabihf`) succeeds, and the same
download then fails only under the parallel fan-out.

## Fix options

- **Pre-add the required rust-std targets serially** before the parallel west
  fan-out (the fixture driver knows the board/target set), so no concurrent
  rustup fetch happens during the parallel stage. Cleanest.
- Or set `RUSTUP_PERMIT_COPY_RENAME` / a per-build `RUSTUP_HOME` / a rustup lock so
  concurrent invocations don't share one downloads dir.
- Or serialize the rustup-ensure step (the rest of the build can stay parallel).

## Impact

Blocks `build-test-fixtures` (hence `test-all`) on any host that doesn't already
have every needed rust-std target pre-installed — i.e. a fresh/CI-image host on its
first run, or a dev host building a new target line (4.4). CI images that pre-bake
all targets don't hit it; fresh hosts do. Orthogonal to phase-258 (surfaced while
running test-all for it).
