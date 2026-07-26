---
id: 291
title: "zpico's zenoh pin (1.7.2) is behind modern rmw_zenoh (jazzy = 1.11.2) — zenoh interop with a stock jazzy peer likely broken"
status: open
type: bug
severity: medium
area: rmw
related: [phase-311, phase-41]
---

## Summary

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
