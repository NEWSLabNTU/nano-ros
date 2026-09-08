---
id: 1247
title: "No RFC-0047 subnode package builds for a freestanding target, so nothing
  measures whether the merged `rclcpp::Node` still fits one"
status: open
type: gap
area: api
related: [phase-427, rfc-0047, rfc-0089, rfc-0044]
---

## Problem

phase-427 W4 merged `nros::ComponentNode` into `nros::Node` and asked, as its
acceptance, that **one of the RFC-0047 subnode packages build for a freestanding
target** — the test of whether the one merged type still fits an image with no
allocator and no STL.

It does not, and it never did. All three consuming `fixtures.toml` rows are
`platform = "linux"`:

- `examples/workspaces/realtime-cpp` (`subnode_pkg`)
- `examples/workspaces/realtime-cpp-subnode-portable` (`subnode_pkg`)
- `examples/native/cpp/component-node-poc`

So the acceptance names a PORT that has to be written, not a check that can be
run. W4 shipped the merge and the check half; this is the port half, split out
deliberately rather than attempted at the end of an already-large item.

## Why the existing coverage does not answer it

The workspace that HOSTS `subnode_pkg` (`examples/workspaces/realtime-cpp`)
already has nuttx, freertos and zephyr rows — which reads like coverage and is
not. Those rows select the `configure`-shape packages through their own launch
files; the subnode package is never in the image. `subnode_pkg/CMakeLists.txt`
carries no platform restriction, so nothing refuses the build — nothing requests
it.

What DOES exist, and is not a substitute:

- `tests/compile/one_node_type_freestanding.cpp` — constructs a node and creates
  a publisher `-nostdinc++` against the ThreadX minimal libcpp. It proves the
  TYPE parses and its freestanding members compile. It does not link an image,
  and it exercises no callback group.
- `sizeof(nros::Node)` is measured at 224 bytes, identical hosted and
  freestanding (`scripts/check-cpp-capability-layout.sh`). That is the layout
  half of "still fits", not the link half.

The gap is specifically: an image that instantiates a derived node with callback
groups, on a target with no allocator, LINKED.

## What "fits" would have to mean

The merged type carries two things a freestanding image pays for
unconditionally:

- the 24-byte error latch (deliberate — it is the error channel a
  `-fno-exceptions` target has instead of a throwing constructor);
- nothing else. The 192-byte timer pool became `nros::NodeWithTimers<N>`, a
  derived template whose default is no pool, so a plain node is unchanged.

A port would measure whether that holds through a real link: whether the
`_in` family's arena registration, `check_declared_depth`'s table lookup and
`create_callback_group` pull in anything the target cannot provide.

## Smallest path

A new `[image.*_subnode]` row pointing at `subnode_system.launch.xml` in
`examples/workspaces/realtime-cpp`, on one freestanding coordinate. Pick the
platform from what the workspace already provisions rather than adding a
toolchain for this.

Note the two SubNode sources now derive `::nros::NodeWithTimers<2>` and call
`create_publisher_in` / `create_timer_in`, so the port starts from code that
already speaks the merged surface — the change is the fixture row and whatever
the link surfaces, not the node.

## Related

- `docs/roadmap/phase-427-one-node-type.md` — W4, where this acceptance was
  split off.
- RFC-0047 — what the subnode packages exercise is several named CALLBACK
  GROUPS on one node, not several named nodes (a correction PR #773 landed).
