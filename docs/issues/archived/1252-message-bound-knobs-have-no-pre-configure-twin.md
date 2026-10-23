---
id: 1252
title: "The message-bound half of the Zephyr chain still has no pre-configure producer, so one re-configure survives and RFC-0094 A3 cannot be met"
status: resolved
area: build, cmake, cli
severity: medium
found: 2026-09-10
related: [1228, 0940, 0965, 0991, 1002, 1119, 1061, 1125, phase-439, phase-403, phase-478]
---

**Home: [phase-439](../../roadmap/phase-439-resolve-before-configure.md)** — homed 2026-10-17; the phase doc already tracks it in its body.

# RESOLVED 2026-10-23 — the bound link is seeded (W6) and held to (W8)

The acceptance's open rows are closed as follows. The fixed-point deletion was
re-scoped by ruling (phase-439 W8, RFC-0094 A3 amended). On a configure
`nros build` started, the message-bound sizes are held to the seed at the end
of the top-level configure, over a recorded set of inputs
(`resolved-message-bounds-inputs.cmake`). The same inputs with a different
answer stops the configure, naming both files; it is measured in vivo by
appending one comment to the seed. Moved inputs converge by the fixed point,
which stays for configures `nros build` did not start (a bare `west build` of a
leaf measured 1 re-configure). `.config`: not compared, because nothing W6 or
W8 changes reaches Kconfig.

# UPDATE 2026-10-23 (W6 landed) — 0 re-configures on the reference image

The design below is built: a byte-for-byte Rust twin of the composer
(`nros_cli_core::message_bound_knobs`, parity-tested against the cmake module
on every case of `tests/cmake-message-bounds-tests.sh`), fed before the
configure from `resolve-deps` over the west app's node packages, the metadata
probe's bound tables and `resolved.cmake`, and seeded into the build dir by
`nros_resolved_seed_message_bounds`. Item 1's "refuse when the closure could
differ" is how it behaves: every input it cannot produce is NO SEED.

Acceptance against the list below, on `demo_bringup:zephyr`, clean dir:

* `Re-running CMake` = **0** -- met;
* `check-knob-delivery <build-dir>` green -- met;
* delivered knobs identical -- met (the final pass's 13 resolved-knob lines);
  `zephyr/.config` -- NOT compared;
* deleting `nros_reconfigure_settle` and the future-mtime arm -- NOT done, and
  not yet possible: an image with no seed (bare `west build`, no `nros sync`)
  still needs the arm. That is phase-439 W8.

# UPDATE 2026-10-23 (later) — phase-439 W6: the descriptor arm is gone; the bound arm needs inputs, not a composer

**The descriptor arm was not about bound tables.** It hashed
`nros/sizing/west-cargo-descriptor.cmake`, which records only the PATH named
to cargo. The module's resolver runs inside `find_package(Zephyr)`, cannot know
the entry's name, and on a clean dir wrote an empty placeholder, so the entry's
path armed the pass. The west road now names one fixed file
(`nros/sizing/west-cargo.toml`, filled by the entry), and cargo reads its
content at build time. Measured on the image below, clean dir: still 1
`Re-running CMake`, and **this issue's arm is the only one left**.

**Measured: the Rust side agrees, and that does not help.** For
`demo_bringup:zephyr` the sizing descriptor and `nros_derive_message_bound_knobs`
state the same answer: subscribed `std_msgs/msg/Int32` = 12, small class 12,
0 large subscribers, take buffer refused because 14 of 32 closure types are
unbounded. But the descriptor is composed during configure, from the same
`nros_message_bounds.json` tables codegen writes in `nros_find_interfaces()`. A
Rust composer exists (`leaf_payload_classes::join`, `leaf_take_buffer::derive`);
INPUTS before configure do not. On the cargo road `nros sync` runs codegen first,
which is why the leaf twin works there. On the cmake/west road codegen runs
inside the configure.

**What a pre-configure producer needs, in order:**

1. **The image's interface closure, without cmake.** On this image the
   transitive `package.xml` `<depend>` closure of the bringup's component
   packages ({std_msgs, builtin_interfaces}) equals the 2 tables cmake
   registered. That is one image; cmake's closure is whatever members
   `find_package`, and nothing forces the two to agree. A producer must REFUSE
   (and leave the configure to derive, as today) when they could differ. It
   must never guess, or the payload classes under-size: the unsafe direction.
2. **The bound rows for that closure.** `rosidl_codegen::bounds` already
   computes them from the parsed interfaces. Stage 3.5 can call it over the
   ament share of the closure without writing generated packages.
3. **One composer.** Render `message_bound_knobs.cmake` in Rust at stage 3.5,
   and turn `nros_derive_message_bound_knobs` into a reader that compares, the
   shape 1228 used for entities. The fragment's text names counts of the
   closure ("14 of 32 types"), so byte equality is the test, and a cmake-side
   append is the regression to guard (phase-439 W5).

Acceptance is unchanged below.

# UPDATE 2026-10-23 — phase-439 W5: two arms remain, not one

Re-measured on the same image, seed present, clean build dir, after the entity
arm was fixed a second time (#1228's 2026-10-23 update): 1 `Re-running CMake`,
and pass 1 fires TWO arms — this one (the message-bound fragment) AND the
sizing descriptor (`cmake/NanoRosSizingDescriptor.cmake`, label "the sizing
descriptor this image names to cargo"). This issue's acceptance therefore needs
the descriptor arm quiet too. Find out first whether it moves only because the
bound tables it composes over arrive mid-configure.

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
