---
id: 1847
title: "live-peer's fixture build runs `cargo build --frozen` for a generated entry whose crates were never fetched, so it dies on `failed to download cmake v0.1.58`"
status: resolved
type: bug
severity: medium
area: [ci, build, cli]
related: [1811, 1758, 1848]
found: 2026-10-12
---

## Measured

`live-peer regression` run 38251106285 (re-run started 2026-10-12 15:38Z,
head `199f34afb`), job 114887202010 `rows whose board is NOT this runner`
(GitHub-hosted), step `Build the fixtures those rows resolve`:

```
error: failed to download `cmake v0.1.58`

Caused by:
  attempting to make an HTTP request, but --frozen was specified
nros build: `cargo build --manifest-path build/posix-zenoh/native_rust_qos_entry/Cargo.toml --config build/posix-zenoh/native_rust_qos_entry/nros-cargo.toml --frozen` exited 101
Error: `nros build demo_bringup:native_rust_qos` exited exit status: 101; no census can be taken without it
error: recipe `build-fixtures` failed with exit code 1
```

`cmake 0.1.58` is in the root `Cargo.lock`. The hosted runner starts with an
empty cargo registry, and nothing in the job fetches the generated entry's
dependency set before `nros build` runs cargo `--frozen`, which forbids the
network.

## Why it shows now

`--frozen` has been in `nros build`'s cargo invocation since September
(phase-383 W3, phase-445 W4b/W5), so it is not the new part. This lane did
not reach the fixture build before: until issue 1811's fixes merged
(#2040, #2049), the same job died earlier, in `just setup`
(`zenoh-pico submodule not found`). So this was masked, not introduced. It
may have been made reachable by phase-485 W2 (`89a843e739`, 2026-10-11:
"a leaf Cargo.lock is seeded from the pin set"), if a seeded lock names
crates no earlier step fetches. **Unconfirmed.**

## What it is not

- Not a network outage: `--frozen` refuses the request before any network
  I/O.
- Not issue 1811 (store-source paths): `just setup` completes in this run.
- Not the transient toolchain download (curl exit 22) on the first attempt
  of this run, which failed at setup.

## What would close it

The fixture build on a fresh hosted runner gets the generated entry's
crates before the `--frozen` build. For example, a `cargo fetch --locked`
on the generated manifest inside `nros build` (offline only after a
fetch), or a lane step that fetches them. Proven by this job getting past
`Build the fixtures those rows resolve`. Keep the guarantee `--frozen`
gives against a silently re-resolved lock; `--locked` plus a fetch keeps
it.

## Resolution: fixed under issue 1848

Issue 1848 was filed in parallel for the same failure and carries the fix.
The `--frozen` build is reached from `scripts/build/census-prepass.sh`:
`nros ws entity-census take --offline` builds the host sibling of a
contracted cross image (here `native_rust_qos` for `zephyr_rust_qos`).
`workspace-fixtures-build.sh` already warmed the cache before its own offline
builds (issue 0967), but the prepass did not. The prepass now asks
`take --plan` which host image it would build and warms that image's cache
with the shared `nros_warm_leaf_cache` (`scripts/build/leaf-cache.sh`). The
build stays `--frozen`. Whether the phase-485 W2 lock seeding contributes
was not checked. The failure reproduces locally against an empty
`CARGO_HOME`, and the warm step clears it.
