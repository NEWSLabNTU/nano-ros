---
id: 2487
title: "`safety` on a backend with no CRC path reports every sample valid: E2E integrity fails open on Cyclone and XRCE"
status: resolved
type: bug
severity: high
area: [safety, rmw, build]
related: [0072, rfc-0028, rfc-0071]
found: 2026-11-13
resolved_in: "fix-2487-safety-fail-open: a declared capability the backend does not carry is refused on every road; IntegrityStatus::is_valid needs a checked CRC; RFC-0028 §7 As built"
---

**Found by** the phase-555 W2 review of RFC-0028. The decision brief is
`docs/roadmap/data/phase-555-w2-rmw-wire-brief.md`, item B5. This issue is
filed ahead of the user's ruling, because it is a safety defect and not a
documentation one.

## The defect

An image that declares `features = ["safety"]` gets per-sample integrity
verdicts. On zenoh those verdicts are real. On a backend with no CRC carrier
(Cyclone DDS, XRCE-DDS), every sample is reported **valid** without any check
being made.

The chain, measured on `main` (`0a4cfcc063` or later):

1. **The capability no-ops on a backend that does not declare it.**
   - `safety` is a capability row with `nros_feature: Some("safety-e2e")` and
     `backend_feature: Some("safety-e2e")`
     (`packages/cli/cargo-nano-ros/src/capability_resolver.rs`, `CAPABILITIES`).
   - `Capability::backend_supports` is true only when the backend's
     `nros-rmw.toml` declares the capability. Only zenoh does:
     `packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml`, `safety = "safety-e2e"`.
   - The resolver's own comment says a capability the backend does not
     declare "no-ops exactly as before".
   - The `nros` half is still turned on: `nros/safety-e2e` enables
     `nros-node/safety-e2e`, `nros-rmw/safety-e2e` and `nros-rmw-cffi?/safety-e2e`
     (`packages/api/nros/Cargo.toml:212-216`).
   - The board halves (`nros-board-*`'s `safety-e2e = ["nros-rmw-zenoh?/safety-e2e"]`)
     are inert without zenoh.
2. **The default `take_validated` vouches for nothing, as if valid.** The
   `nros-rmw` trait default returns `IntegrityStatus { gap: 0, duplicate:
   false, crc_valid: None }` (`packages/core/nros-rmw/src/traits.rs:3770-3781`).
   The cffi seam that Cyclone, XRCE and uORB sit behind initialises each
   subscription's status to the same value
   (`packages/rmw/cffi/src/lib.rs:1027-1031`). Only a backend with a CRC path
   ever overwrites it, and none of those three has one.
3. **`None` counts as valid.** `IntegrityStatus::is_valid` is `self.gap == 0
   && !self.duplicate && self.crc_valid != Some(false)`
   (`packages/core/nros-rmw/src/safety.rs:80-81`).
4. **The application is told the sample was checked.** A listener that reads
   the verdict sees a valid sample. That is the Rust
   `create_subscription_for_callback_name_with_safety`, the C/C++
   `nros_cpp_subscription_register_validated`, and the C API's
   `rclc_executor_add_subscription_raw_validated`. Its `/safe_ok` counter (the
   safety workspace's C listener) climbs exactly as it does on a checked
   backend. Gap and duplicate detection are not computed there either: the
   default `take_validated` always returns `gap: 0` and `duplicate: false`.

`nros/safety-e2e` does not forward to the backend by itself (`nros/Cargo.toml:212-216`
names no backend), which is RFC-0028 §7's "no change to the backend" plan.
Issue 0072 fixed the zenoh lowering and left this open in its own text:
"Other backends (cyclonedds, xrce) — do they carry a `safety-e2e` CRC path?
If not, the axis silently no-ops the CRC there too; document or gate."

## Does any in-tree image build it today?

Measured 2026-11-13 on `main`: **no**.

- Every system that declares the `safety` capability names `rmw = "zenoh"`:
  - `examples/workspaces/safety/src/demo_bringup/system.toml`: `[system] rmw`
    and `[image_defaults] rmw` are `"zenoh"`, and the Zephyr image uses
    `prj-zenoh.conf`.
  - `examples/linux/c/safety-listener/.../system.toml`: `rmw = "zenoh"`.
  - `examples/linux/cpp/safety-listener/.../system.toml`: `rmw = "zenoh"`.
- No `fixtures.toml` row pairs `safety` with Cyclone or XRCE:
  `grep -nE 'safety' examples/fixtures.toml | grep -iE 'cyclone|xrce'` is
  empty.
- The test bins that enable `nros/safety-e2e` directly
  (`nros-tests/bins/declarative-safety-listener`, `nros-bench/wcet-cycles-qemu`)
  enable zenoh's backend feature beside it.

Issue 2423 (#3641) made a workspace image's `[image.<id>] features` reach the
facade and the board. A capability a backend does not declare still no-ops, so
that change does not add a fail-open image: it only makes the declaration
reach the crate graph on the backends that have the path.

So nothing in the tree reports an unchecked sample as valid today. But nothing
stops a user from writing `rmw = "cyclonedds"` (or `"xrce"`) under
`features = ["safety"]`. That builds, boots and reports every sample valid.
The safety workspace's own `system.toml` comment says "pointing another `rmw`
at this bringup is what would change the answer". In practice it would change
the answer to a false "valid".

## What a fix has to decide

This is RFC-0028's brief item B5. The options are:

- **(a) Document it.** State in RFC-0028 and the book that E2E is zenoh-only.
- **(b) Refuse it.** A `safety` declaration on a backend that does not declare
  the capability becomes a build-time refusal naming the backend, at the same
  place the capability is resolved. The refusal message carries no issue id.
- **(c) Build a carrier.** Design a DDS carrier, such as an inline QoS or a
  payload footer. RFC-0028 §4 option B rejected that for interop.

The reviewer recommends **(b) plus (a)**: an unsupported backend that says
"valid" is the worst of the three outcomes. A complementary runtime half
would make the default `take_validated` answer "not checked" rather than
valid, for example by having `is_valid()` refuse `crc_valid: None` when the
image declared `safety`. That half is also the user's call.

## Acceptance (once ruled)

- A system with `features = ["safety"]` and `rmw = "cyclonedds"` or
  `"xrce"` either fails to build with a message naming the backend (b), or
  can no longer report an unchecked sample as valid.
- The zenoh safety cells stay green.
- A negative fixture row records the refusal: exit code plus stderr, per the
  "must it fail to compile" rule.

## Resolution (2026-11-13)

User ruling (relayed by the manager session): **(b) plus (a)**, with the
runtime half as defence in depth.

**(b) Refuse.** A declared capability that has a backend half and that the
image's backend does not declare in `[rmw.capabilities]` is now a build-time
refusal on every road that lowers capabilities. It used to be omitted with a
warning.

- Cargo road (workspace facade and leaf selection) and the Zephyr west Rust
  road: one predicate, `backend_link::undeclared_capability_refusal`, called
  from `facade::derive` and `west_app`. The message names the backend and the
  backends that carry the capability, and carries no tracker id.
- cmake road: `nros_feature_set` (`cmake/NanoRosFeatureSet.cmake`), reached by
  both `nros-c` and `nros-cpp`, so `-DNROS_SAFETY_E2E=ON` by hand is refused
  too. `WARNING` became `FATAL_ERROR`.
- Zephyr C/C++ road (`zephyr/CMakeLists.txt`): the same, now for every API
  rather than C++ only.
- The class, not the site: the predicate covers every capability with a backend
  half, so `link-tls` on a backend without it is refused the same way. The plan
  check's warning (`planner::check_plan_file`) now says the build will refuse.

**(a) Document.** RFC-0028 §7 is replaced by an "As built" section (E2E is
zenoh-only; what a subscriber is told), §8's matrix is corrected, and the
book's safety page says the build refuses `safety` on Cyclone DDS or XRCE-DDS.

**Defence in depth.** `IntegrityStatus::is_valid()` is true only for
`crc_valid == Some(true)`. `None` (no CRC: a non-safety publisher, or a backend
default that checked nothing) is no longer valid. The Verus model
(`integrity_is_valid`, proof 5) and the production/ghost correspondence test
moved with it. The C and C++ surfaces were already strict (`crc_valid == 1`).
Behaviour change: a safety subscriber on zenoh now reports a sample from a
publisher built without safety as not valid. It is still delivered.

**Acceptance.**
- Negative row `cmake_safety_unsupported_rmw_refused`
  (`cmake-configure-verdict`): configure exits 1 with the refusal, asserted by
  `cmake_platform_matrix::safety_on_an_rmw_without_crc_is_refused_at_configure`.
- Unit tests: `backend_link::a_capability_the_backend_does_not_carry_is_refused`
  (cyclonedds and xrce, `safety` and `link-tls`, no tracker id in the message)
  and `facade::a_capability_the_images_rmw_does_not_carry_is_refused` (refused
  on cyclonedds and xrce, selected on zenoh).
- The zenoh safety cells are unchanged: the C and C++ listeners already counted
  only `crc_valid == 1`, and the Rust listener's samples carry a CRC.
