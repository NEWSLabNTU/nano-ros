---
id: 2766
title: "Zephyr callback tracing calls sys_trace_app_marker, but the CTF event that defines it is a patch only the island carries; other users get no marker events"
status: resolved
type: bug
severity: low
area: [zephyr, tracing]
related: [phase-478, issue-2641]
found: 2026-11-20
resolved: 2026-11-22
---

GitHub: NEWSLabNTU/nano-ros#4008

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md) I33** (found by the safety island).

## What happens

nano-ros's Zephyr callback trace sink, `nros_zephyr_trace_marker`
(`zephyr/nros_platform_zephyr_shims.c`, installed by the Zephyr sched-task
entry), calls `sys_trace_app_marker` through a weak reference; callback
tracing (`packages/core/nros-node/src/executor/callback_trace.rs`) and issue
2641's `rclc_trace_marker` / `rclcpp::trace_marker` all reach it. Stock
Zephyr has no such event. It comes from a CTF patch that the consumer carries,
not nano-ros:

- `patches/zephyr/0002-ctf-app-marker-event.patch`: the `app_marker` CTF event;
- `patches/zephyr/0001-ctf-trace-k_thread_priority_set.patch`: a CTF fix for
  `k_thread_priority_set`;
- `patches/zephyr/0003-tracing-allow-an-out-of-tree-backend.patch`: lets an
  image name its own tracing backend;

applied on every Zephyr build by the island's `scripts/apply-zephyr-patches.sh`,
because `nros setup board` rewrites `zephyr/`. Without 0002 the weak
reference is null, so the sink skips the call: any other nano-ros user with
CTF tracing on builds an image whose callback and application markers emit
nothing, with no warning. (The island's own trace code calls the symbol
directly, so its trace image fails to link without the patch.)

## Wanted

nano-ros carries these patches in `zephyr/patches/` beside the ones it
already ships (`native-sim-ipproto-ip-4.4.patch` and the others), and
`nros setup board` applies them, so a Zephyr CTF image links and emits the
marker event with no consumer patch. The island then drops its copies and
`apply-zephyr-patches.sh`. Upstreaming the event to Zephyr is a separate step.

## Fix (in review; open until the acceptance build)

nano-ros's patch set carries both events now, as anchor-edit appliers like
the rest of `scripts/zephyr/` (nothing reads `zephyr/patches/*.patch`, so a
copy there would apply nowhere):

- `scripts/zephyr/ctf-app-marker-patch.sh <ws> <id>`: the island's 0002 event,
  same name and fields (`app_marker`: `marker_id`, `arg`), so a reader keyed
  on the metadata reads either. The id is per line: **0x70 on 3.7** (the
  island's, free in its 8-bit space) and **0xF000 on 4.4**, where ids are
  16-bit and 0x70 is upstream's `gpio_port_set_bits_raw_exit`. The script
  refuses an id the tree already uses, one too wide for its id type, and a
  tree carrying the event in some of its three files but not all. It adds a
  prototype beside the definition, which the island's copy lacked.
- `scripts/zephyr/ctf-priority-set-patch.sh <ws>`: 3.7 only, the one-line
  mapping the island's 0001 README settled on (its earlier
  `sys_port_trace_k_thread_priority_set` define is unreached and not carried).
  4.4 has upstream's mapping already.

On a 3.7 copy, the result matches the island's 0001 + 0002 applied to the same
tree in everything but comments, the prototype and that unreached define.
The island's 0003 (an out-of-tree tracing backend) is not part of this issue:
nothing in nano-ros needs it, and its exit plan retires it on 4.x.

Measured on host 241 (2026-11-20), on copies of the 3.7 and 4.4 store
workspaces: each line's patch set exits 0 and again on a re-run; Zephyr's
`samples/subsys/tracing` (`prj_native_ctf.conf`, native_sim/native/64) with
one injected `sys_trace_app_marker(7, 42)` builds and runs, and `channel0_0`
holds the payload `07000000 2a000000` after id byte `0x70` (3.7) and after
`00 f0`, i.e. 0xF000 as a little-endian u16 (4.4). Not yet run: a nano-ros
sched-task image with CTF on (the shim's weak call end to end), and 3.7's
`thread_priority_set` events.

## Acceptance

- A Zephyr image with CTF tracing on and callback tracing enabled, built on a
  fresh `nros setup board` checkout with no consumer patches, has a CTF
  stream holding the `app_marker` events (checked with babeltrace or the
  in-tree CTF reader).
- The island's patch directory is gone on its nano-ros-next branch.

## Acceptance cell (2026-11-21, in review)

The first acceptance item is a CELL, so it stays verified:
`(ZephyrNativeSim, C, Zenoh, RealtimeSchedTasks, Workspace) + [Ctf]`, answered
by `tests/zephyr_ctf_markers.rs` over the `workspace-zephyr-c-realtime-ctf`
row: realtime-c's `[image.zephyr_ctf]`, the C sched-task image with
`prj-ctf.conf` (CTF on the synchronous posix backend) and
`NROS_TRACE_CALLBACKS`. C because only the C sched task runner installs the
sink. The test:

- reads `ZEPHYR_BASE` from the build's `CMakeCache.txt` and requires that
  tree's TSDL metadata to declare `app_marker` (the patch set is applied);
- boots the image with its own router and working directory, lets both sched
  tasks run 2 s, and DECODES the whole `channel0_0` against that metadata
  (`nros_tests::ctf`: an unknown id or a stream ending mid-event is an error;
  only a tail torn by the kill is dropped);
- requires `app_marker` events for callback register (16), at least 20 starts
  (18), and per-handle start/end (19) pairing.

The decoder's own cases (8- and 16-bit ids, arrays, unknown id, truncation)
are unit tests, and it parses this host's 3.7 and 4.4 metadata (74 and 242
events). Not yet run: the image build and the cell (host 241); the manager's
scratch run saw marker(7,42) on 4.4 and no image on 3.7, so 3.7 is the line
to watch. The second acceptance item (the island drops its copies) is the
island's change.

The shim's comment named a decoder, `scripts/parse-zephyr-ctf.py`, that is not
in the tree; it now names the in-tree reader, `nros_tests::ctf`.

## Resolved (2026-11-22)

The acceptance cell ran on host 241 at main `4406b7f4e5`. The image built with
no consumer patch (rc 0 on both lines), and
`zephyr_ctf_markers::zephyr_ctf_stream_holds_the_callback_markers` passes on
Zephyr 3.7 (5.4 s) and 4.4 (2.5 s). That is 3.7 too, the line this issue said
to watch. The stream decodes against the tree's own TSDL metadata and holds
the register, start and per-handle start/end `app_marker` events.

The second acceptance item, the island dropping its patch copies, is the
island's change on its own branch, not nano-ros's.
