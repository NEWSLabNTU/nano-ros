---
id: 2444
title: "`nros build --offline` does not reach the metadata probe's cargo; only `NROS_OFFLINE=1` does"
status: open
type: bug
area: cli, offline, ux
purpose: [industrial, education]
related: [phase-551]
found: 2026-11-11
---

**Home: [phase-551](../roadmap/phase-551-starter-image.md)** — W2's offline acceptance. Related: archived issue 2324 (the hint) and issue 2197 (`NROS_OFFLINE` → `CARGO_NET_OFFLINE`).

## Measured

In the locally built starter image (release `0.6.0-nros8`, everything provisioned, `docker run --network none`):

- `nros new demo --board linux --template talker --lang rust --rmw zenoh && nros build --offline` fails at the metadata-mode harness: `failed to get 'nros' as a dependency of package 'demo_node' … Could not resolve host: index.crates.io`.
- The same build with `NROS_OFFLINE=1` and no flag succeeds in 14.6 s, and the talker publishes to a local `rmw_zenohd`.

So the flag's own help text ("stages 1–4 touch no network") is false on the first stage that runs cargo.

## Cause (read on main)

There are two spellings of "offline" and they reach different consumers:

- `$NROS_OFFLINE` is mapped once, in `main`, to `CARGO_NET_OFFLINE` (`rust_toolchain::expose_offline_to_cargo`), so every cargo the process spawns sees it, the metadata probe's included.
- `nros build --offline` (`cmd/build.rs`, `args.offline`) is threaded by hand into the cargo commands `build.rs` itself assembles (`--frozen`, `--offline`), the entity census and configure. `orchestration/metadata_build.rs` takes no offline input and sets nothing, so the probe's cargo resolves online.

## Next

Make the flag one input to the same rule as the env: `--offline` should set what `$NROS_OFFLINE` sets, before any stage runs, so the two cannot reach different sets of cargos. Fixing only `metadata_build.rs` leaves the next cargo-spawning stage with the same gap.
