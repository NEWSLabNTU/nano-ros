---
id: 74
title: provision sccache in the SDK tier so the compiler cache is on by default
status: resolved
type: enhancement
area: build
related: [phase-251, rfc-0014]
resolved_in: phase-251
---

## Resolution

Provisioned sccache through the SDK system (source-build path):

- **`nros-sdk-index.toml`** — added `[tool.sccache]` + `[tool.sccache.source]`
  (`cargo install` from `mozilla/sccache` `v0.8.2`). Source-only: `dist.<host>` left
  out until a prebuilt asset is seeded on `nano-ros-sdk`, so `nros setup --tool sccache`
  falls back to the source recipe (verified: `--dry-run` resolves
  `→ ~/.nros/sdk/sccache/0.8.2-nros1`).
- **`activate.sh` + `activate.fish`** — the SDK-store PATH loop now also adds a bin dir
  holding `sccache` (verified: with `~/.local/bin` removed, a staged store sccache
  resolves on PATH after `source ./activate.sh`).
- **`just workspace install-sccache`** — added + chained into `just workspace setup` (the
  `base` tier). Delegates to `nros setup --tool sccache`; skips when sccache is already on
  PATH; non-fatal on source-build failure (sccache only speeds builds).
- `just doctor` already reports `[OK] sccache` / the absence hint (phase-251 follow-up).

Once on PATH, the justfile's `RUSTC_WRAPPER` (rustc) and the zephyr fixture's
`CMAKE_{C,CXX}_COMPILER_LAUNCHER=sccache` (the safe per-build C cache, already wired in
`scripts/build/zephyr-fixture-leaves.sh`) both light up automatically — ~46 % host /
~22 % embedded faster clean rebuilds.

**Residual (not blocking, optional):** seed a prebuilt `dist.<host>` asset on
`NEWSLabNTU/nano-ros-sdk` so first setup downloads instead of source-building; and the
host-only `CC`/`CXX="sccache cc"` knob (native cmake/cargo C builds) left as a documented
opt-in given the cross-`CC` risk. Neither is required for the cache to be on by default.

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
