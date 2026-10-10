---
id: 1535
title: "The MIXED Zephyr image SEGVs on its first publish — `CffiPublisher::poll_status_events` reads an unusable vtable"
status: resolved
type: bug
area: examples, zephyr, rmw
severity: high
found: 2026-09-28
related: [1288, 0616, 1566, 1568, 1787, 0436, phase-470, phase-477, rfc-0031]
resolved_in: "issue 1566's storage fix (#1566/#1568); compile-time C storage-size guard in component.h (PR #1891)"
---

# The image dies before it prints anything

`examples/workspaces/mixed` `[image.zephyr]` — the native_sim image that links
a C talker, a C++ listener AND a Rust heartbeat node — boots and takes
SIGSEGV on the FIRST timer tick, before any nano-ros output reaches the
console:

```
WARNING: Using a test - not safe - entropy source
*** Booting Zephyr OS build v3.7.0 ***
timeout: the monitored command dumped core
```

Its sibling `examples/workspaces/cpp` `[image.zephyr]`, built and run the same
way in the same session, runs to completion (talker publishes, the in-image
listener receives), so the harness is not the subject.

# Where

```
Thread 3 "zephyr.exe" received signal SIGSEGV, Segmentation fault.
0  nros_rmw_cffi::CffiPublisher::poll_status_events () at src/lib.rs:3565
     3565	        let Some(take) = self.vtable.publisher_take_event else {
1  nros_rmw_cffi::{impl#15}::publish_raw () at src/lib.rs:4031
2  nros_cpp::publisher::nros_cpp_publish_raw () at src/publisher.rs:205
3  on_tick (ctx=0x549840 <__nros_c_inst_c_talker_pkg>)
     at examples/workspaces/mixed/src/c_talker_pkg/src/Talker.c:32
4  nros_cpp::timer::nros_cpp_timer_create::{closure#0} () at src/timer.rs:54
5  nros_node::executor::arena::timer_try_process<...>
6  nros_node::executor::spin::Executor::spin_once_capturing ()
...
12 nros::board::ZephyrBoard::run_components<int (*)()> (locator="tcp/127.0.0.1:7447")
14 main () at <build>/zephyr_entry_nros_main_generated.cpp:77
```

The faulting read is `self.vtable.publisher_take_event` — a field load off the
publisher's vtable pointer. So the publisher handed back by the backend
carries a vtable this TU cannot dereference: either it is not the vtable the
backend installed, or the `CffiPublisher` this TU sees has a different layout
from the one that was written.

That is the failure mode the **single-runtime invariant** exists to prevent —
"one Rust staticlib, ONE `nros-rmw-cffi` REGISTRY", the reason
`NROS_WS_RUST_NODE_DIRS` bundles a workspace's Rust node into the
`nros_ws_runtime` umbrella instead of linking it beside plain nros-cpp. This
image is the only one in the tree that exercises it, which is presumably why it
is the only one that fails.

Two things measured that narrow it, and one that does not:

* **The umbrella IS built and linked.** `<build>/nros_ws_runtime/` and
  `<build>/nros-rust-ws-nros_ws_runtime/` both exist, and the linked ELF holds
  313 `heartbeat` strings. So this is not "the bundling did not happen".
* **The backend IS registered**, once: the generated
  `nros_app_register_backends.c` holds exactly
  `(void)nros_rmw_zenoh_register();`.
* **It is NOT about which application built it** — see below.

# It is NOT phase-470 W5.b3

Found while migrating this image to a GENERATED west application (issue 1288,
phase-470 W5.b3). The pre-migration, HAND-WRITTEN
`examples/workspaces/mixed/src/zephyr_entry` was recovered from `origin/main`,
built fully in the same worktree, and run the same way: **identical SEGV, same
boot output, same point.** Their merged `zephyr/.config` files are also
byte-identical. So the generated application reproduces the hand-written one's
behaviour exactly, including this, and the defect predates the migration.

# Why no lane reported it

`workspace-zephyr-mixed` is `skip_probe = true` and is built by the west lane
(`just zephyr build-fixtures --include-workspace-entry`), whose e2e consumer is
`tests/entry_e2e.rs`'s `(ZephyrNativeSim, Mixed, EntryPubsub)` cell — a
`Runtime` cell in `matrix::CELLS`. A Runtime cell that has been red for a while
has no signal capacity (the CLAUDE.md "a red CI lane answers one of two
questions" entry), so this needs a `just nightly-triage` window read before
anyone concludes when it broke.

# Reproducing

```sh
# a router on the image's baked locator
rmw_zenohd -c <(echo '{ mode: "router", listen: { endpoints: ["tcp/127.0.0.1:7447"] } }')
nros build zephyr --workspace examples/workspaces/mixed -- -d <build-dir>
gdb -batch -ex run -ex bt --args <build-dir>/zephyr/zephyr.exe --seed=7448
```

`LD_LIBRARY_PATH` must carry `<ros-prefix>/opt/zenoh_cpp_vendor/lib` or the
router itself SEGVs for an unrelated reason (issue 0774).

# Resolved (2026-10-10, phase-477 W7)

## Cause: the C talker's publisher buffer was 72 bytes short, and the Rust node's storage sat right after it

This is issue 1566, seen from a different image. `NROS_C_PUBLISHER_STORAGE_SIZE`
was the literal **560**. `nros_cpp_publisher_create` takes no size and writes a
`CppPublisher`, which is `NROS_PUBLISHER_SIZE + sizeof(void*)` = 632 + 8 =
**640** bytes on native_sim/zenoh. In the mixed image the linker put the Rust
heartbeat's per-class slot store directly after the C talker's instance:

```
0000000000563a40 0000000000000238 b __nros_c_inst_c_talker_pkg            (568 B)
0000000000563c78 0000000000000130 b ..._NROS_COMPONENT_rust_heartbeat_pkg_SLOT_STORE
```

So the order of events was:

1. the talker's `configure` creates its publisher, writing 640 bytes into a
   568-byte instance: the last 72 land in the heartbeat's slot store;
2. the heartbeat's install (the NEXT component) initialises its slot store,
   zeroing those 72 bytes;
3. the first timer tick publishes. `CffiPublisher::poll_status_events` loads
   `self.vtable` from offset **0x268 (616)** of the talker's buffer — past the
   instance, inside the slot store — reads **0**, and dereferences it.

That is why this image and no other failed: it is the only one with a Rust
node's `.bss` storage after a C talker. It is also why the hand-written entry
did the same: it compiled the same `component.h`.

### Evidence

- The faulting load, at the crash: `mov 0x268(%rdi),%rax` with
  `rdi = 0x563a40 <__nros_c_inst_c_talker_pkg>` and `rax = 0`.
- REPRODUCED byte for byte. Tree `85ae1d156` (the commit that filed this issue)
  built in a scratch worktree runs clean as checked out. Putting the pre-1566
  literal back (`#define NROS_C_PUBLISHER_STORAGE_SIZE 560`) and rebuilding
  incrementally shrinks the instance from `0x288` to `0x238`, and the image dies
  with this issue's backtrace exactly: `poll_status_events` (lib.rs, the
  `publisher_take_event` line) ← `publish_raw` ← `nros_cpp_publish_raw` ←
  `on_tick (ctx=<__nros_c_inst_c_talker_pkg>)` ← `nros_cpp_timer_create::{closure}`.
- Timeline. The issue was found on 2026-09-28. The 1566 fix (`4274aed26`, the
  per-build size) landed on 2026-09-29, and 1568 (`7512c743c`) removed the
  literal fallback that same day. The filing commit is from 2026-10-01 and
  already contains both, so the issue recorded a measurement taken before the
  fix it needed. On `origin/main` (`5210fd053`) the image publishes and its
  in-image listener receives (`[c_talker_pkg] sent: N` / `Received: N`, 11 of 11
  in 11 s).

The vtable-ABI (0331) and knob-split (0135) suspects were checked and ruled
out: the two halves agree on `NROS_PUBLISHER_SIZE` (632 in both per-build
headers), and the backend registers once.

## What this change adds: a guard, plus a second defect at the same seam

**Guard: a short buffer no longer compiles.** The 1566 formula cannot be short,
but the macro can still be overridden with `#ifndef`, and a literal is one edit
away. `component.h` now **asserts** each of its four storage sizes (publisher,
action server, service client, action client) against the per-build size the
matching `nros_cpp_*_create` writes. It uses the existing `NROS_STATIC_ASSERT`
from `nros/serialization_format.h`, so the assertion still has only one
spelling. Negative control: compiled against the mixed image's own
per-build headers, `-DNROS_C_PUBLISHER_STORAGE_SIZE=560` (the pre-fix value)
fails with `static assertion failed: "NROS_C_PUBLISHER_STORAGE_SIZE is smaller
than the CppPublisher nros_cpp_publisher_create writes"`, and the default
compiles. `just check c` now runs that expected-failure probe (with 8 bytes,
so the result does not depend on the host's publisher size), next to the
existing `nros_cpp_ffi.h` + `component.h` cross-include TU.

**Second defect: issue 1787 (filed and resolved in the same change).** While
ruling out a layout split, a second latent corruption turned up on the same
path. The generated C++ entry handed `rclcpp::global_handle()`, an nros-cpp
`CppContext*`, straight to the Rust node's `__nros_component_<pkg>_install`,
and the Rust side cast it to `*mut Executor`. That works only while rustc
happens to place the executor first in a `repr(Rust)` struct. It does today, by
luck. Pinning the tag at offset 0 SEGVs this same image inside the heartbeat's
`register`. Issue 1787 has the details.

## Sweep

```sh
# every C storage macro now carries an assert
git grep -n "NROS_STATIC_ASSERT(NROS_C_" packages/api/nros-c/include/nros/component.h
# every literal C storage size (must be none)
git grep -nE "define NROS_C_[A-Z_]+_STORAGE_SIZE +[0-9]" -- 'packages/**/*.h'
```

## Verification

- `examples/workspaces/mixed` `[image.zephyr]`, built with the fix under
  `rmw_zenohd`: the talker publishes and the in-image listener receives;
  `__nros_component_rust_heartbeat_pkg_install` returns 0. The fixture build
  and `entry_e2e` result are recorded in the PR.

## Verification on the final tree (2026-10-11)

Rebased onto `main`, with the header as merged:

- `just ci gate`: all six steps green (`check::build` 52 min, `test-unit`,
  `test-lane-contracts`).
- `entry_e2e` `zephyr/mixed/entry_pubsub`: fixture rebuilt through the west
  lane (`NROS_ZEPHYR_FIXTURE_FILTER=workspace-entry-mixed just zephyr
  build-fixtures`), and the cell RAN and passed. It was the one cell of 18
  with a built fixture here; the other 17 report unmet preconditions, not
  results.

The fixture's freshness probe reported `DEGRADED … examined 0 input(s)`, so
its FRESH verdict measured nothing. The binary was built minutes earlier from
this tree, so the result stands, but the probe gap is its own defect:
issue 1794.
