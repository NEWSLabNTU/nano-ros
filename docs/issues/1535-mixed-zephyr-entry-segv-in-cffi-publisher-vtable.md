---
id: 1535
title: "The MIXED Zephyr image SEGVs on its first publish — `CffiPublisher::poll_status_events` reads an unusable vtable"
status: open
type: bug
area: examples, zephyr, rmw
severity: high
found: 2026-09-28
related: [1288, 0616, phase-470, rfc-0031]
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
