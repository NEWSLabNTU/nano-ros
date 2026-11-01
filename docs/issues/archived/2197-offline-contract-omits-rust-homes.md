---
id: 2197
title: "The air-gapped contract cached NROS_HOME only: rustup's homes and cargo's network were outside it"
status: resolved
type: bug
area: cli, book
purpose: industrial
related: [rfc-0116, phase-523]
found: 2026-11-01
resolved: 2026-11-01
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** W4, the offline install probe.

## Problem

`book/src/getting-started/vendored-tree.md` "Air-gapped / mirrored CI" told an
industrial adopter that the CLI, the SDK root and everything `nros setup`
fetches land under `${NROS_HOME:-~/.nros}`, and that `NROS_OFFLINE=1` forbids
the fetch. The first run of the offline probe track (`just probe offline`: the
bare-metal chapter online to stage the store, then again in a container with
`--network none`) found two places where that was not true.

1. **The Rust toolchain is not under `NROS_HOME`.** `nros setup` installs it
   with rustup, which keeps it in `~/.rustup` and `~/.cargo`. A host that cached
   only `NROS_HOME` ran `nros setup mps2-an385-baremetal` and failed:

   ```
   Error: provision a Rust toolchain (every board's runtime is compiled by cargo)
      0: download https://static.rust-lang.org/rustup/archive/1.29.1/x86_64-unknown-linux-gnu/rustup-init
   ```

2. **`NROS_OFFLINE=1` did not reach cargo.** With the Rust homes cached,
   `nros sync` ran cargo, which tried to update a git dependency its cache
   already held:

   ```
   unable to update https://github.com/NEWSLabNTU/ros-launch-manifest.git?tag=v0.2.4
   failed to resolve address for github.com: Temporary failure in name resolution
   ```

   The flag was read only by the index fetch in `nros setup`.

## Fix

- The book's contract names the Rust homes as the one cache outside
  `NROS_HOME` (`${RUSTUP_HOME:-~/.rustup}`, `${CARGO_HOME:-~/.cargo}`, the
  crate cache included).
- `nros` sets cargo's own `CARGO_NET_OFFLINE=true` for every cargo it starts
  when `NROS_OFFLINE` is set and the user has not set `CARGO_NET_OFFLINE`
  (`rust_toolchain::expose_offline_to_cargo`, called from `main` beside
  `expose_cargo_bin`).
- The offline probe's stage 2 keeps exactly what the corrected contract says to
  cache, so a step that needs anything else fails there.

## Resolution (2026-11-01)

Both fixes above landed with phase-523 W4, and the offline probe passes:
`just probe offline` reached the bare-metal chapter's `Hello World` lines in
stage 2 with `--network none`. Whether the Rust toolchain should move under
`NROS_HOME` so the contract is one directory again is a design question, kept
in phase-523's "Left" section rather than here.
