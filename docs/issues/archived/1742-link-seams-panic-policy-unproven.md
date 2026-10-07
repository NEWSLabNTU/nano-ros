---
id: 1742
title: "Most `nros_{platform,board}_link_app` seams apply no panic policy, and only one is held to it"
status: resolved
type: bug
area: [build, cmake, tooling]
severity: low
found: 2026-10-07
related: [issue-0719, issue-1735, issue-1614, issue-0689, issue-1753, rfc-0077]
resolved_in: "carriers apply the policy they create (issue 1742)"
---

## What was measured

Folding `check-image-paths-apply-policy` into `check-cmake-image-policy`
(issue 1735) made the gate read each `function()`/`macro()` scope. Its first
run flagged `nano_ros_node_register` in `cmake/NanoRosNodeRegister.cmake`: the
NuttX and ThreadX typed-entry CARRIERS each `add_executable` an image, link
`NanoRos::NanoRos(Cpp)`, and apply no policy themselves. Instead they hand the
image to `nros_platform_link_app(...)`.

Whether that delegation is sound depends on the seam. Of the 17 tracked
`nros_{platform,board}_link_app` definitions under `cmake/`, three call
`nros_apply_panic_policy` (`cmake/NanoRosEntry.cmake`,
`cmake/platform/nano-ros-nuttx.cmake`,
`cmake/board/nano-ros-board-rv-virt-threadx.cmake`), and `REQUIRED_SEAMS` in
the gate holds exactly ONE of them (NuttX) to it. So a ThreadX carrier on
`threadx-linux` reaches a seam that applies nothing, and no gate asks.

## What the gate does today

`check-cmake-image-policy` treats a scope that calls a link seam as having
DELEGATED the policy. That reading is correct exactly when the seam applies it.
That is a statement about every seam, and today it is true for three of 17.

## Needs a ruling

Either every seam an image can reach WITHOUT going through `nano_ros_entry()`
applies the policy (add them to `REQUIRED_SEAMS` and fix the ones that do not),
or the carriers in `nano_ros_node_register` apply it themselves before calling
the seam. Hosted `threadx-linux` links `std` and may not need a
`#[panic_handler]`, so whether the gap is LIVE depends on that board. Nothing
measured it here.

## Resume checkpoint (2026-10-08) — a record; the Resolution below closes it

Work paused for token budget. **PR #1804** (`fix/1742-carrier-panic-policy`) implements the agreed design:
- the carriers in `cmake/NanoRosNodeRegister.cmake` take `PANIC platform|halt|own` (default `platform`) and call `nros_apply_panic_policy` after `add_executable`;
- link seams stay policy-free;
- `check-cmake-image-policy` requires the policy where an image is created.

At pause the PR was open with auto-merge armed and CI pending. To resume:
1. Check that #1804 merged. If it was ejected, rebase it, re-run `just setup-cli`, and re-arm.
2. Verify by mutation: delete the carrier's `nros_apply_panic_policy` call; the gate must fail.
3. Confirm the threadx-linux (hosted, `std`) finding recorded in the PR.

Dropping `std` from threadx-linux is a separate, parked question.

## Resolution

Ruled: the panic policy belongs to whoever CREATES the image (RFC-0077's
amendment, "who links the final image"). Vocabulary `platform|halt|own`,
default `platform`, applied by the one implementation
`nros_apply_panic_policy(<policy> <context>)`.

- **Carriers.** The four typed-entry carriers in `nano_ros_node_register`
  (NuttX, ThreadX, FreeRTOS, native) now create their image through ONE
  helper, `_nros_node_register_carrier_image(<target> <pkg_sym> <panic> <srcs>)`,
  which calls `nros_apply_panic_policy` right after `add_executable`.
  `PANIC platform|halt|own` is a keyword of `nano_ros_node_register`,
  `nano_ros_add_node` and `nros_components_register_node` (beside `DEPLOY`,
  the other nano-ros extension that makes a register create an image).
- **Seams are plumbing.** NuttX's `nros_platform_link_app` no longer applies a
  hard-coded `platform` (which made a carrier's own `PANIC` a second voice that
  could only agree or FATAL). NuttX's own-lane verification is unchanged:
  `nros-nuttx.cmake` still declares `NROS_ENTRY_PANIC_APPLIED platform`, and a
  carrier asking for `halt` there FATALs with the lane's own sentence
  (measured). The other two former callers are creators, not seams
  (`nano_ros_entry`, `nros_threadx_rv64_rust_app`), and keep their calls.
- **Gate.** `check-cmake-image-policy` requires the policy in every scope that
  CREATES an image; handing the image to a link seam no longer counts.
  `REQUIRED_SEAMS` is retired and replaced by the inverse rule: a
  `nros_{platform,board}_link_app` definition must NOT apply the policy.
  Failures name the scope (`function(_nros_node_register_carrier_image)`).

| mutation | rc |
| --- | --- |
| delete the carrier helper's `nros_apply_panic_policy` call | 1, names `cmake/NanoRosNodeRegister.cmake (function(_nros_node_register_carrier_image))` |
| re-add `nros_apply_panic_policy(platform …)` to NuttX's `nros_platform_link_app` | 1, names the seam |
| clean tree | 0 |

Self-test rows (normal path, 14): a carrier delegating to a seam is flagged,
a carrier applying its own `PANIC` before the seam is not, a seam applying the
policy is flagged, a policy-free seam beside an applying top level is not.

### Was the gap live? Yes, on bare-metal ThreadX

Deleting the carrier's call and configuring an `rv-virt-threadx` carrier drops
`panic-platform` from the cargo command
(`--features=ros-humble,rmw-zenoh-cffi,alloc,platform-threadx`) and `nros-c`
fails with `#[panic_handler] function required, but not found`. With the call
it compiles; the final link then hits an unrelated, previously masked
`__libc_fini_array` relocation error, filed as issue 1753.

### Build evidence

- **threadx-linux carrier** (C talker, `DEPLOY threadx`): both cargo commands
  carry `...,std,platform-threadx,panic-platform`; links, boots, and publishes
  `sent: 0..6` against a local `rmw_zenohd`.
- **NuttX carrier** (`qemu-armv7a-nuttx`, `DEPLOY nuttx`): the ARM ELF builds.
  NuttX's ending is NOT on a cargo command line — it is baked into
  `nros-nuttx-ffi`'s committed manifest (`panic-platform`), and the carrier's
  call verifies it (`PANIC halt` → FATAL naming the manifest).

### threadx-linux links `std` — the finding

`platform` composes: `nros-c`'s handler is `cfg(not(feature = "std"))`, so on a
std tier `panic-platform` is INERT and std's own handler ends the image. It
does not route to `nros_platform_panic` there. `halt` does NOT compose: it is
the `panic_halt` crate, and with `std` rustc refuses the pair as E0152
"duplicate lang item `panic_impl`" inside `nros-c` (measured on threadx-linux).
`nros_apply_panic_policy` now refuses `halt` at configure when the target's
`CORROSION_FEATURES` hold `std`, for every creator (entry or carrier), naming
the reason. Dropping `std` from threadx-linux (under consideration) would make
`platform` live there and `halt` legal; not done here.
