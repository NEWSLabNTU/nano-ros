---
id: 621
title: "A vendored nano-ros puts its 272 example packages into the CONSUMER's
  package index, and the first duplicate ends their build"
status: resolved
type: bug
area: cli, examples
related: [phase-358, issue-0506, rfc-0026, rfc-0066]
---

## Symptom

A consumer that vendors nano-ros as a subdirectory cannot build. From
`nano-ros-rt-eval`, which carries it as the submodule `nano-ros/`:

```
error: nros::main!: build_pkg_index: duplicate pkg name `demo_bringup`
       in workspace `/home/aeon/repos/nano-ros-rt-eval`:
       `…/nano-ros/examples/templates/c-and-cpp-mixed-workspace/src/demo_bringup`
   and `…/nano-ros/examples/templates/multi-node-workspace/src/demo_bringup`
  --> src/freertos_entry/src/main.rs:25:22
   |
25 | nros::main!(launch = "demo_bringup");
```

Both paths named are inside the dependency. Neither is the consumer's
`demo_bringup`, which is the one the macro was asking for.

## Cause

`build_pkg_index` walks the workspace root for every `package.xml` and requires
the names to be globally unique within that walk. It prunes build/VCS
directories and honours `COLCON_IGNORE` / `.nros-ignore`, and nano-ros shipped
neither at its root.

nano-ros contains **272 package names, 28 of them duplicated** — and that is
correct by design, not a defect to clean up. Every copy-out workspace has its
own `demo_bringup` (×18), `native_entry` (×12), `talker_pkg` (×8),
`zephyr_entry` (×7): each is a separate project a user copies out whole
(RFC-0026), and RFC-0066's naming rules deliberately give the same role the
same name in every workspace. They are unique *within a workspace*, which is
the only scope where uniqueness means anything.

The bug is not the duplicates. It is that a consumer's root becomes an ancestor
of all of them, so nano-ros's internal namespace is spliced into the
consumer's — and the error then names directories the consumer does not own.

## Fix

`.nros-ignore` at the nano-ros repo root.

The semantics are exactly right, and they come from the walker's own shape:
`build_pkg_index`'s filter returns `true` for `entry.depth() == 0` before any
marker is read. So the file is **never consulted when nano-ros IS the workspace
root** (nano-ros's own discovery is untouched) and prunes at depth 1 when it is
nested under someone else's root.

Verified both directions rather than argued: the consumer's build gets past
package discovery, and nano-ros's own example/fixture builds are unaffected
because the marker sits at a depth their walks never inspect.

## Also fixed: three packages named `native_talker`

Found while sweeping for this, and wrong independent of the vendoring:

| directory | declared name |
| --- | --- |
| `examples/native/rust/talker` | `native_talker` ✓ |
| `examples/native/rust/custom-transport-talker` | `native_talker` ✗ |
| `examples/native/rust/custom-transport-listener` | `native_talker` ✗ |

A copy-paste, and unlike the 28 above these are in ONE flat namespace with no
workspace boundary between them — a real collision, and a *listener* declaring
itself a talker. Renamed to `native_custom_transport_{talker,listener}`,
matching the `native_xrce_serial_{talker,listener}` pair.

Fixing these alone was not enough — it just moved the consumer's failure to the
next duplicate, `demo_bringup`. Recorded because the intermediate result is the
useful part: it showed the duplicates were not the disease.

## Not

* Not a request to make the 272 names unique. Doing so would break RFC-0066's
  naming rules and every copy-out workspace's readability, to satisfy a scope
  that should never have contained them.
* Not covering `colcon` itself — a consumer running colcon at their root would
  still descend into a vendored nano-ros. `COLCON_IGNORE` at the root would fix
  that too, but colcon's exemption rules for the base path were not verified
  here, and shipping an unverified marker that could hide nano-ros's own
  workspaces is the worse trade. Left for whoever hits it.

## Found by

Phase-358 W3. The transport-band cells run on `nano-ros-rt-eval`'s FreeRTOS
QEMU lane, and taking them against current code means bumping that repo's pin —
which is when this surfaced. The harness pinned `c10371776`, from before the
example packages that collide were added.
