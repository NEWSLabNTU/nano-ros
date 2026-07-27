---
id: 291
title: "zenoh interop with a stock jazzy peer — the blocker was the RIHS01 keyexpr tail, NOT the zenoh version pin (zpico 1.7.2 interops with jazzy 1.11.2)"
status: resolved
type: bug
severity: medium
area: rmw
related: [phase-311, phase-304, phase-41, 292]
---

## RESOLUTION (2026-07-27) — the version gap was a red herring

Live investigation **refuted** the original premise. The zenoh WIRE protocol is
version `0x09` on BOTH sides (`zenoh-protocol::VERSION` in the 1.7.2 submodule +
`Z_PROTO_VERSION 0x09` in zenoh-pico 1.7.2; zenoh froze the wire at 1.0, so
1.11.2 is also `0x09`). **Proven:** the pinned zpico 1.7.2 opens a session with
a stock jazzy `rmw_zenohd` **1.11.2** router (pub AND sub), and — once the
keyexpr matched — a `rmw_zenoh_cpp` peer received nano-ros samples.

The REAL blocker was the **RIHS01 type-hash tail of the data keyexpr**. jazzy
`rmw_zenoh_cpp` (0.2.10 source studied) builds the data keyexpr
`<domain>/<topic>/<type>/<type_hash>` and subscribers declare it CONCRETELY (no
hash wildcard), so delivery needs the hash to match EXACTLY. nano-ros emitted
`TypeHashNotSupported` because the interop **fixtures were built `ros-humble`**
(the keyexpr type-hash tail is cfg-gated on `ros-iron`/`ros-jazzy` in
`nros-rmw-zenoh`, and humble codegen bakes the placeholder). The product +
RIHS01 engine were already correct (phase-304): building the fixture with
`ros-jazzy` bakes the real `RIHS01_b6578…` (== live jazzy) and the keyexprs
match.

**Fix (no zenoh version bump):**
- The `examples/native/rust/*` now select the ROS edition like the RMW — a
  `ros-humble`(default)/`ros-iron`/`ros-jazzy` passthrough feature (forwarding to
  `nros` + `nros-rmw-zenoh?`), instead of hardcoding `ros-humble`.
- `just ros_editions build-e2e-fixtures <distro> zenoh` regenerates msgs for the
  edition (real RIHS01) + builds `--features "rmw-zenoh ros-<edition>"`.
- `ros_editions_zenoh.rs` lane: zpico node ↔ `rmw_zenohd` ↔ `rmw_zenoh_cpp` peer,
  both directions. **jazzy: 5/6 green** (pub/sub both, service both, action
  client). The ROS→nano action-SERVER direction is a separate graph-token gap,
  tracked as **#0292**. Completes phase-304 W4-remaining (the wire lane).

Residual (below) kept for the record; the **version divergence** is now FUTURE
WORK, only relevant if a future edition bumps the zenoh PROTO version past `0x09`
(1.7→1.11 did not). iron + humble ship no `rmw_zenoh_cpp` at all, so jazzy is the
only edition this axis touches today.

## Summary (original — premise since refuted)

nano-ros pins **zenoh-pico 1.7.2** (`packages/zpico/zpico-sys/zenoh-pico/
version.txt`) and its vendored `rmw_zenoh` fork pins **zenoh-c/cpp 1.7.1**
(`third-party/zenoh/rmw_zenoh/zenoh_cpp_vendor/CMakeLists.txt`: "VCS_VERSION to
1.7.1 commits", pkg 0.1.8). But a modern ROS 2 distro ships a much newer
`rmw_zenoh_cpp`:

| | rmw_zenoh pkg | zenoh version |
| --- | --- | --- |
| nano-ros pinned | 0.1.8 | 1.7.1 (zpico 1.7.2) |
| jazzy apt (`ros-jazzy-rmw-zenoh-cpp`) | 0.2.9 | **1.11.2** |

**1.7.1 → 1.11.2 is a 4-minor-version gap.** Zenoh does not guarantee wire-
protocol stability across that range, so a nano-ros zpico node is very likely
**wire-incompatible with a default jazzy `rmw_zenoh_cpp` node** (the stock setup a
jazzy user has). The `TypeHashNotSupported`/RIHS01 keyexpr work (phase-41) is
orthogonal — it does not help if the zenoh transport itself can't handshake.

## How it surfaced

phase-311 (RMW × ROS-edition interop matrix) tried to add a zenoh lane to the
multi-edition harness. Inspecting the sources showed the version gap above.
The jazzy image's `libzenohc.so` strings report `1.11.2`; the pinned vendor
CMake declares 1.7.1.

## Impact

- **Real jazzy users** running stock `rmw_zenoh_cpp` (1.11.2) cannot interop with
  nano-ros zenoh nodes (pending a compat test to confirm the break, but the
  version gap makes it near-certain).
- The multi-edition harness's zenoh lane (phase-311 W5) is blocked: building the
  pinned-1.7.1 overlay would test a *non-default* jazzy zenoh, not a stock peer.

## Options

1. **Bump zpico + the vendored rmw_zenoh to a modern zenoh** (~1.11) that matches
   the current LTS distros. Biggest interop win; the pin exists (CLAUDE.md) for
   an OLDER `rmw_zenoh_cpp` compat target, which has moved on. Re-verify the
   pinned-1.7.2 gotchas (zpico tx path, session config) against the bump.
2. **Track per-distro zenoh versions** as part of the ROS-edition profile
   (RFC-0056) — humble/iron/jazzy each shipped a different rmw_zenoh/zenoh, so a
   single zpico pin cannot match all. Pick the pin per the primary interop
   target (probably the current LTS) and document the others as unsupported.

Cross-ref: phase-311 (the harness that surfaced this; zenoh lane deferred here),
phase-41 (RIHS01 keyexpr — the orthogonal type-hash half of the zenoh profile).
