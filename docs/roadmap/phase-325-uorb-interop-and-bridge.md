# Phase 325 — uORB interop: direct, and bridged to any RMW

**Status (2026-07-31): Draft.** Not started.
**Implements:** RFC-0026 (example layout), RFC-0048 (cmake consumption).
**Successor to:** [phase-316](phase-316-example-tree-axes.md) W4, which carried
the decisions but not the work — scoping showed W4 is a phase, not a work item.
**Informed by:** issues 0351 (proofs that observe the wrong thing), 0356
(`px4_e2e` targets a retired tree), 0288.

## Goal

Two demonstrations, making two distinct claims:

| | claim | proven against | falsified by |
| --- | --- | --- | --- |
| **direct** (W2) | nano-ros speaks PX4's in-memory format, so no serialization happens at all | a **stock, unmodified PX4 module** | a stock module cannot read the topic |
| **bridge** (W3) | nano-ros carries uORB traffic out to any RMW it supports, selected at build time | a **real ROS 2 node** | ROS 2 cannot see it, or only one backend works |

Both acceptances name a **foreign peer**, and that is not incidental. A
nano-ros↔nano-ros test passes identically whether the encoding is right or
wrong, because both ends share the bug — issue 0351's shape, hit twice during
phase-316. The stock PX4 module and the real ROS 2 node ARE the measurement; a
demo that drops them proves nothing it claims to prove.

## Why uORB is the special one

Decided by the maintainer, and load-bearing in the code rather than aspirational:

| | every other backend | uORB |
| --- | --- | --- |
| wire bytes | CDR encoding of the message | the PX4 C struct, verbatim |
| type identity | ROS type name + type hash | `ORB_ID(<topic>)`, a static descriptor |
| serialization cost | encode + decode per sample | none — the payload IS the struct |
| who can read it | another nano-ros / ROS 2 endpoint | **any stock PX4 module**, unmodified |

`publisher_publish_raw` checks `len >= meta->o_size` and hands the caller's bytes
straight to `orb_publish`. `publisher_create` ignores `type_name`, `type_hash`,
`qos` and `domain_id`, resolving the topic through `nros_rmw_uorb_register_topic`
to a `const struct orb_metadata *`. Everywhere else nano-ros interoperates by
speaking a wire protocol; here it interoperates by **sharing PX4's in-memory
type**.

That is also the cleanest statement of why `examples/px4/cpp/uorb/` looked like an
RMW path level and was not one (phase-316): uORB is not a transport choice, it is
the absence of a transport.

## What is already true

Worth stating precisely, because three artifacts look like PX4 integration and
the tree reads as though this is solved:

| artifact | what it actually exercises |
| --- | --- |
| `nros-rmw-uorb/tests/register_smoke.cpp` | drives the RMW **vtable directly**, stubbing `nros_rmw_cffi_register` AND the uORB ABI. Never touches `nros-cpp`. |
| `packages/testing/nros-px4-register-check/` | compiles the backend inline against **real PX4 headers** and calls `nros_rmw_uorb_register()`. Proves it LINKS. Does not link `nros-cpp` — the weak `register_fallback.c` exists precisely so it need not. |
| `integrations/px4/module-template/nano_ros_app.cpp` | the node code is a **comment**: *"Replace this comment block with NodeBuilder / Publisher calls"*. |

So: **no nano-ros node has ever been constructed on the uORB backend.** The
backend's proven surface stops below the node API. `examples/README.md` called the
register-check "the canonical PX4 uORB surface" — true about linking, easy to
misread as usage.

Two things that ARE proven and remove risk:

- **`publish_raw` / `subscription_take` are already public** on both the C and C++
  APIs. The direct example needs no new data-plane machinery.
- **Two live backends in one image works.** `examples/bridges/tt-zenoh-to-cyclonedds`
  does `nros_rmw_zenoh::register()` + `nros_rmw_cyclonedds_sys::register()` then
  `Executor::open_with_rmw("zenoh", &cfg)` and opens a second session.
  `open_with_rmw` takes the backend by **name**, so build-time selection needs
  only a cargo feature choosing which `register()` compiles in and which name
  string is passed.

## The actual gap: consumption, not a platform port

phase-316's note said "there is no `cmake/platform/nano-ros-px4.cmake`, and every
other platform has one". That is true and **the wrong diagnosis** — recorded here
because a wrong diagnosis points at the wrong fix, which is this session's
recurring lesson.

Platform modules are consumed by nano-ros's OWN root `CMakeLists.txt`
(`cmake/platform/nano-ros-${NANO_ROS_PLATFORM}.cmake`, resolved at
`CMakeLists.txt:116`). A PX4 module is built by **PX4's** cmake via
`px4_add_module()` and never enters that file. And SITL is an ordinary host
x86_64 process, so the platform shim it needs is `posix`, which already exists.

The gap is a **consumption path**: how a `px4_add_module()` target links
`libnros_cpp.a` + the posix platform shim + the uORB backend. That is RFC-0048
territory — `find_package(nano_ros)` → `_nros_bootstrap` → `add_subdirectory` of
the nano-ros root with `NANO_ROS_PLATFORM=posix` and `NROS_RMW=<selected>` — not a
new platform port.

**Real PX4 boards (NuttX, cross-compiled) are explicitly out of scope.** Both
demos run on SITL. A board port is the `nuttx` platform plus a cross toolchain and
is its own phase; nothing here should pretend to deliver it.

## Work items

### W1 — a PX4 module can consume nano-ros

- [ ] **W1.1** Prove `find_package(nano_ros REQUIRED)` configures inside a PX4
      SITL build, with `NANO_ROS_PLATFORM=posix`, and yields a target a
      `px4_add_module()` can link. Expect friction where PX4's module factory and
      nano-ros's `add_subdirectory` import disagree about flags/targets; record
      what actually breaks rather than predicting it here.
- [ ] **W1.2** Wrap the result as ONE helper — working name
      `nros_px4_add_module()` — under `integrations/px4/`, so a module author
      writes one call. Not a copy of `px4_add_module`'s argument surface: forward
      to it.
- [ ] **W1.3** Retire the module-template's comment-block placeholder in favour of
      the helper, so the template compiles what it documents. A template whose
      body is `// Replace this comment block` is how the gap stayed invisible.

**Acceptance:** a PX4 SITL build produces a module that links `libnros_cpp.a` and
starts. No node behaviour yet — that is W2.

**Receipt:** `nm` on the module archive shows nano-ros C++ symbols resolved, and
the module runs from the pxh shell without an unresolved-symbol abort.

### W2 — the direct demo: nano-ros ↔ a stock PX4 module

- [ ] **W2.1** A nano-ros node inside a PX4 module that publishes a real PX4
      topic:
      `nros_rmw_uorb_register_topic("/<topic>", "<ros_type_name>", ORB_ID(<topic>))`,
      then `publish_raw((const uint8_t *)&msg, sizeof msg)` with `msg` a
      `<uORB/topics/*.h>` struct. The message type comes from PX4's headers, NOT
      from `nros generate-*`.
- [ ] **W2.2** The subscribe direction, reading a topic a stock PX4 module
      publishes.
- [ ] **W2.3** Lands at `examples/px4/cpp/firmware/` — which this creates.
      phase-316 W3.1 deliberately left the dir uncreated rather than empty.
- [ ] **W2.4** A test that observes the exchange **from the PX4 side**: `listener
      <topic>` in the SITL shell, or an upstream module that already subscribes
      it. Assert on that output.

**Acceptance:** a message crosses between a nano-ros node and an unmodified PX4
module, with no serialization step on either side, and the test reads it from the
PX4 end.

**Explicitly NOT acceptance:** nano-ros subscribing its own publication. That
passes identically with a correct and a broken struct layout — it measures the
loopback, not the interop.

### W3 — the bridge: uORB → the build-time-selected RMW

- [ ] **W3.1** A PX4 module holding two sessions: uORB inward
      (`nros_rmw_uorb_register()`), and outward on the RMW chosen at build time —
      cargo `rmw-*` features / `-DNROS_RMW=<backend>`, the same knob every other
      example uses. `Executor::open_with_rmw(<name>, …)` already takes the backend
      by name; the feature picks the `register()` call and the name string.
- [ ] **W3.2** ONE path, no `<rmw>/` level and no backend pair in the directory
      name. This is phase-316's rule applied to the thing that used to break it:
      the outward backend is a build-time CHOICE, not a directory axis.
- [ ] **W3.3** Build it against **at least two** backends (zenoh + one of
      xrce/cyclonedds). One backend does not demonstrate selection; it
      demonstrates a hardcoded bridge with extra ceremony.
- [ ] **W3.4** A test with a **real ROS 2 node** subscribing the bridged topic.
      `packages/testing/nros-tests/src/ros2.rs` + `ros_env.rs` already spawn real
      ROS 2 peers for the interop cells — reuse that, do not invent a second way.

**Acceptance:** a stock PX4 module's uORB topic reaches a real ROS 2 subscriber
through the bridge, and the same source builds against a second backend.

**Not claimed:** zero-copy. The serialization uORB avoids returns at the RMW
boundary, necessarily. W2 demonstrates the zero-copy property; W3 demonstrates
reach. Conflating them would overclaim.

### W4 — the existing bridges encode their backend pair in the directory name

Not required by W1–W3, and deliberately last.

`examples/bridges/tt-zenoh-to-cyclonedds` and `tt-zenoh-to-xrce` differ only in an
outward backend the build could have chosen, with both backends named as hard
crate deps. That is the per-RMW axis phase-316 removed from paths, surviving in a
name.

- [ ] **W4.1** Decide whether they collapse to one `tt-zenoh-to-rmw` with the
      egress selected at build time, as W3's bridge is. Record the answer here
      before touching them.

**Why it matters:** if only the uORB bridge is built the right way, it reads as an
inconsistency rather than a rule, and the next bridge copies whichever neighbour
it happens to open first.

## Risks

- **W1 is the real unknown.** W2 and W3 are ordinary example code once a PX4
  module can link nano-ros; W1 is the first time anyone has tried. If it turns out
  hard, the honest move is to say so and stop — not to route around it with a
  demo that skips `nros-cpp` (which is exactly what the register-check does, and
  why this gap survived three phases).
- **Cold SITL builds are ~10 min.** Iterating on W1 means paying that repeatedly.
  Budget for it; do not shorten the loop by testing something smaller that does
  not link `nros-cpp`, because the linking IS the question.
- **`just px4 test-sitl` is currently red** — issue 0356: `px4_e2e.rs` targets
  `examples/px4/rust/uorb/{talker,listener}`, retired by phase-277 W7. Resolve
  0356 first (delete it, or let W2 supersede it) so this phase's receipts are not
  read against a lane that already fails.
- **Concurrent sessions.** Other agents are active; land each W in small pushed
  steps.

## Receipts to collect

| Step | Receipt |
| --- | --- |
| W1 | PX4 SITL module links `libnros_cpp.a`; `nm` shows resolved nano-ros symbols; module starts from pxh |
| W2 | a stock PX4 consumer (`listener <topic>`) prints a message published by the nano-ros node, asserted by a test |
| W3 | a real ROS 2 subscriber receives a stock PX4 module's uORB topic through the bridge; same source builds against a second backend |
| W4 | decision recorded here before any edit to `examples/bridges/tt-zenoh-to-*` |

## Provenance

Decisions carried from phase-316 W4, recorded there on 2026-07-31 and unchanged:

- **W4.1** — the uORB example demonstrates interop with existing PX4 features; it
  skips serialization so upstream PX4 nodes understand the message format. uORB is
  the special one.
- **W4.3** — the bridge's outward side is the build-time RMW knob, not a fixed
  backend, and the far end is a real ROS 2 node.
