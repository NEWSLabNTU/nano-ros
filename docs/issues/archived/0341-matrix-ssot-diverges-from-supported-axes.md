---
id: 341
title: "The test-matrix SSoT diverges from the supported axes: uORB cannot be expressed at all, and the zephyr Cpp/Cyclonedds Qos cell is satisfied by a Rust/Zenoh test"
status: resolved
type: bug
severity: medium
area: testing
related: [issue-0327, issue-0352, rfc-0051]
resolved_in: 2fabffd33
---

## Finding (deep audit C,E 2026-07-28 — E5/E6, lead-verified)

The deep run did the cell-by-cell cross-read of ARCHITECTURE §2 × `matrix.rs` ×
`fixtures.toml` that the quick pass could not. Two concrete divergences.

### 1. uORB is unrepresentable in the matrix

`packages/testing/nros-tests/src/matrix.rs:90` — the `Rmw` enum defines only
`Zenoh`, `Cyclonedds`, `Xrce`.

But **uORB is claimed supported**: ARCHITECTURE §2 lists
`rmw-{zenoh,xrce,cyclonedds,uorb}`, and there is a real crate
(`packages/px4/nros-rmw-uorb`) plus a real example (`examples/px4/cpp/uorb`).

So no uORB cell can even be *written*, let alone covered — the axis is declared in
the architecture doc and absent from the structure that is supposed to enumerate it.
A missing lane at least shows up as a gap; an inexpressible one cannot.

### 2. A declared cell is satisfied by a different cell's test

`matrix.rs:608` declares:

```rust
cell(ZephyrNativeSim, Cpp, Cyclonedds, Qos, Interop, Runtime),
```

The only runtime test of that shape is
`packages/testing/nros-tests/tests/qos_zephyr_ros2_interop_e2e.rs` — and **verified by
reading it**, it boots the **Rust** `ws-qos-rust` Zephyr entry
(`build_zephyr_workspace_rust_qos_entry`) over **zenoh-pico → `rmw_zenoh_cpp`**, i.e.
`(ZephyrNativeSim, Rust, Zenoh, Qos, Interop)`.

Net effect, both directions wrong:

- a **Cpp / Cyclonedds** zephyr QoS-interop cell is asserted covered by nothing;
- the **Rust / Zenoh** coverage that genuinely exists is unmodelled.

And the drift is invisible: `matrix_fixture_coverage.rs` checks that cells have
fixtures, not that a cell's declared (lang, rmw) matches what its test actually runs.

## Fix

1. Add `Uorb` to the `Rmw` enum (with a documented carve-out row if no lane can run
   in CI — an expressible-but-carved-out cell is honest, an inexpressible one is not).
2. Correct the zephyr QoS cell to `(ZephyrNativeSim, Rust, Zenoh, Qos, Interop,
   Runtime)` to match reality, and file/keep a gap row if Cpp/Cyclonedds coverage is
   actually wanted.
3. Strengthen `matrix_fixture_coverage.rs`: assert that the fixture a cell resolves to
   belongs to the cell's declared **lang and rmw**, not merely that some fixture
   exists. Without that, this class recurs on the next hand-added cell — which is the
   same "gate narrower than its rule" pattern recorded in the quick run's report.

## Relationship to #327

#327 covers the **edition** axis being outside `Cell` entirely. This issue is about
the axes that *are* in `Cell` disagreeing with reality. Same SSoT, different failure:
#327 is a missing dimension, this is wrong data in the existing dimensions.


## Progress (2026-07-30)
**Defects 1 + 2 — FIXED (2fabffd33).** uORB is now expressible: `Rmw::Uorb` +
`PlatformId::Px4` + a documented CarveOut cell (PX4-SITL only, no CI runner). The
`(ZephyrNativeSim, Cpp, Cyclonedds, Qos, Interop)` cell is corrected to
`(ZephyrNativeSim, Rust, Zenoh, …)` (what `qos_zephyr_ros2_interop_e2e` actually
runs), with a CarveOut for the never-covered Cpp/Cyclonedds shape. Gated: the
matrix injectivity/uniqueness/gap-reason unit tests + the matrix⊆⊇fixtures
coverage gate pass with the new platform/rmw.

**Defect 3 — SPUN OUT to #352, this issue RESOLVED (2026-07-31).** Strengthening
the coverage gate to catch a cell whose declared (lang, rmw) disagrees with what
its test *runs* needs a cell→test binding: `Kind::Interop`/`Bridge` cells use
ephemeral peers (and nano sides built by the west leaves lane / `build-e2e-fixtures`,
not fixtures.toml rows), so there is no coordinate to check them against. The real
fix is the same as #327 defect 2 — make the interop/ros_editions runtime tests
consume `matrix::CELLS` directly, so a cell's (lang, rmw) drives the fixture it
builds and the drift class becomes unrepresentable. Confirmed the phase-318 /
RFC-0061 `ci_lane` tier machinery does NOT shortcut it (`coords()` derives from the
cell's own fields, so a lying cell emits a lying coord). That refactor is tracked as
**#352**; defects 1+2 shipped in 2fabffd33, so #341 closes.
