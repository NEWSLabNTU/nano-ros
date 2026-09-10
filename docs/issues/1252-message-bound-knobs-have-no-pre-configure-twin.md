---
id: 1252
title: "The message-bound half of the Zephyr chain still has no pre-configure producer, so one re-configure survives and RFC-0094 A3 cannot be met"
status: open
area: build, cmake, cli
severity: medium
found: 2026-09-10
related: [1228, 0940, 0965, 0991, 1002, 1119, 1061, 1125, phase-439, phase-403]
---

# What is left of the fixed point, MEASURED on a real image

Issue 1228 closed the ENTITY link: `nros build`'s stage 3.5 seed and the
mid-configure producer now render the entity-inventory fragment byte for byte,
so `nros_reconfigure_on_change` no longer arms over it. What did NOT change is
the pass count, and this issue records why — with the numbers.

Measured on `demo_bringup:zephyr` from `examples/workspaces/cpp`
(`native_sim/native/64`, zenoh, Zephyr 3.7), clean west build dir each time:

| run | resolve seed | `Re-running CMake` | arms in pass 1 |
| --- | --- | --- | --- |
| no resolve phase (control) | — | **2** | bounds, entity |
| before 1228's fix | yes | **1** | bounds, entity |
| after 1228's fix | yes | **1** | **bounds only** |

The seed already removed one pass (2 → 1) before this wave, which phase-439 W2
could not measure and did not claim. 1228 removed the second ARM but not a pass,
because both arms fire in the SAME pass 1 and one re-configure discharges both.

## The remaining arm, and why it needs its own design

`${CMAKE_BINARY_DIR}/nros/message_bound_knobs.cmake` is written at the end of
`nros_find_interfaces()` and read by `nros_resolve_knobs()` inside
`find_package(Zephyr)`, which runs earlier. On pass 1 the reader finds the
placeholder `nros_message_bounds_seed_knobs_file()` wrote, the producer writes
the real answer, the bytes differ, and issue 0991's future-mtime arm buys the
pass in which the reader sees it. That is the last link, and

    A3 — the fixed point is gone. No lane re-derives a knob during configure;
    `nros_reconfigure_settle` and the future-mtime arm are deleted, and Zephyr
    converges in one pass.

cannot be met until it closes.

**The entity link's fix does not carry over.** `nros_derive_message_bound_knobs`
is a pure-CMake composer over per-package fragments that CODEGEN produces during
the configure, so there is no Rust twin for stage 3.5 to call and nothing for a
seed to copy. The bound of a type is nevertheless a property of a DECLARATION —
the `.msg`/`.srv`/`.action` files — not of a compiled artifact, so a
pre-configure producer is possible in principle. What it costs is the question
this issue exists to answer:

* who computes a type's bound before any codegen has run (a `nros ws
  message-bounds --model` verb over the same interface packages `nros sync`
  already resolves?);
* the JOIN. The bound inventory prices a TYPE; the entity inventory says which
  types this image RECEIVES. Stage 3.5 already has the second half — the
  fragment it writes carries `NROS_ENTITY_SUBSCRIBED_TYPES` and
  `NROS_ENTITY_RECEIVED_TYPES` — so the join is reachable there, which is the
  encouraging part;
* whether the pure-CMake composer then becomes a READER (one composer, RFC-0094's
  actual rule) or stays a producer with a seed in front of it (two composers
  that must agree byte for byte, which is what 1228 had to enforce for entities
  and what `check-…` has no gate for beyond the Rust unit test).

## Acceptance

The same shape 1228's measurement used, and it is affordable — this host builds
the image in about three minutes:

* `Re-running CMake` goes to **0** on a clean build dir for
  `demo_bringup:zephyr`, with `nros build`'s seed present;
* `python3 scripts/check-knob-delivery.py <build-dir>` green before and after;
* the delivered knob values and the image's `zephyr/.config` byte-identical
  before and after — the control phase-392 W5's withdrawn causal claim exists to
  force;
* only then `nros_reconfigure_settle` and the future-mtime arm in
  `cmake/NanoRosReconfigure.cmake` are deleted, and their tests
  (`tests/cmake-reconfigure-tests.sh`) go with them.

## Reproduction

```
# a private west workspace whose `nano-ros` module is the tree under test
mkdir -p tmp/west-1252 && cd tmp/west-1252
ln -s <zephyr-workspace>/{zephyr,modules,bootloader,tools} .
ln -s <this checkout> nano-ros
mkdir .west && cp <zephyr-workspace>/.west/config .west/

# then, from examples/workspaces/cpp with ZEPHYR_BASE / ZEPHYR_SDK_INSTALL_DIR set
nros build zephyr --zephyr-workspace tmp/west-1252 -- \
    -D_NANO_ROS_CODEGEN_TOOL=<checkout>/packages/cli/target/release/nros \
    -DCONF_FILE="prj.conf;prj-zenoh.conf;<checkout>/cmake/zephyr/native-sim-line-3.7.conf"
```

`grep -c 'Re-running CMake'` on the output is the number; `grep "nros: this
image's"` names which fragment armed.

Note the west workspace has to be private: the shared one's `nano-ros` module is
a symlink to another checkout, so a build run there measures a tree that is not
the one under test (and its build dirs belong to CI — issue 1166).
