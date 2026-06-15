---
id: 74
title: provision sccache in the SDK tier so the compiler cache is on by default
status: open
type: enhancement
area: build
related: [phase-251, rfc-0014]
---

## Problem

The justfile already wires a compiler cache —
`export RUSTC_WRAPPER := \`command -v sccache 2>/dev/null || true\`` — so every
`cargo` invocation under any `just` recipe shares an sccache cache **when sccache is on
PATH**. But sccache is **not provisioned** by `nros setup` / the SDK tier, so on a fresh
machine it is absent and `RUSTC_WRAPPER` is empty: builds are uncached.

Measured impact (native rust talker, `cargo clean` each; see
`docs/development/build-ux-audit.md`):

| build | total |
| --- | --- |
| cold (no cache) | 30.4 s |
| ccache (C only) | 19.9 s |
| sccache (rustc + C) | **16.3 s** (~46 % faster) |

So provisioning sccache roughly halves clean / CI / config-switch rebuilds, and the
wiring to *use* it already exists — only the binary is missing. `just doctor` now flags
its absence (commit on phase-251 follow-up), but that is a hint, not a fix.

## Direction

1. **Provision sccache as an SDK tool** (RFC-0014): add a `[tool.sccache]` entry to
   `nros-sdk-index.toml` with `dist.<host>` release assets + `[tool.sccache.source]`.
   Upstream is `github.com/mozilla/sccache` (prebuilt `x86_64/aarch64` linux + macos
   tarballs exist). The SDK store expects the nano-ros `tar.zst` + `bin/` layout, so the
   asset likely needs repackaging onto the `NEWSLabNTU/nano-ros-sdk` releases (same
   pattern as `zenohd`, `qemu`, the gcc toolchains) and a pinned `sha256`. Then
   `activate.sh` puts `~/.nros/sdk/sccache/<ver>/bin` on PATH and the existing
   `RUSTC_WRAPPER` lights up automatically.
2. **Tier placement** — put it in the `base` tier (every build benefits) unless cache
   disk cost argues for an opt-in tier.
3. **C-side cache (optional, separate)** — host C builds (the zenoh-pico `zpico-sys`
   compile, ~17 s cold) need `CC`/`CXX="sccache cc"`, which the rustc `RUSTC_WRAPPER`
   does not cover. Keep this **opt-in** (e.g. an `NROS_CC_CACHE` knob): it only wraps
   *host* compiles — cross toolchains set their compiler explicitly via toolchain files /
   `cc`-crate target defaults / `.cargo` linker — but forcing `CC` globally is a small
   cross-build risk, so it should be a conscious toggle, not a default.

## Acceptance

- `nros setup` (base tier) installs sccache; `activate.sh` puts it on PATH.
- A fresh `just <plat> build-*` uses sccache automatically (`sccache -s` shows hits on a
  second build); `just doctor` reports `[OK] sccache`.
- The `NROS_CC_CACHE` opt-in (if added) wraps host C builds without affecting any cross
  (zephyr / esp32 / freertos / nuttx) build.
