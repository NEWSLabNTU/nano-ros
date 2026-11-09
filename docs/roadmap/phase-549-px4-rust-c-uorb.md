# Phase 549 -- PX4 in-firmware modules in Rust and C, not only C++

**Status (2026-11-10). NOT STARTED.** Opened to record the gap the
three-language ruling leaves on PX4: an in-firmware nano-ros module on PX4 can
be written only in C++ today. Work items and acceptance only; no design is
chosen and no work is claimed. Each work item is claimed with
`just claim phase-549-wN` before it starts.

Depends on:

- [phase-542](phase-542-px4-supported-platform.md) for the supported C++ road this
  phase mirrors. Its W5 (the `px4-sitl` daily lane) is the lane the new cells join.
- [RFC-0039](../design/0039-px4-integration-architecture.md), the PX4 integration design. A
  Rust or C module road is an amendment to it, written in W0.

---

## Why

The book's PX4 pages say "C++ only", and the PX4 SITL starter (#3420) is
C++ only. Three things make it so:

- **The uORB backend is C++.** `nros-rmw-uorb`
  (`packages/rmw/uorb/nros-rmw-uorb/src/*.cpp`) is C++, because PX4's uORB API is
  C++ (`uORB::Publication`, the `px4::WorkItem` callback glue). Its public
  surface is already C ABI: `nros_rmw_uorb_register()` and
  `nros_rmw_uorb_register_topic()`
  (`include/nros_rmw_uorb.h`, `include/nros_rmw_uorb_registry.h`).
- **The module entry is generated as C++.** `nros_px4_add_module(... BRINGUP ...)`
  generates the entry with `--lang cpp`, one `px4::ScheduledWorkItem` per sched
  task (`integrations/px4/include/nros_px4/sched_task_work_item.hpp`), and links
  `libnros_cpp.a`.
- **The examples are C++.** `examples/px4/cpp/{firmware,bridge}`. The only
  Rust PX4 examples are XRCE companions (`examples/px4/rust/companion`), which run
  BESIDE PX4, not in its firmware.

What exists to build on:

- **`px4-rs`** (`third-party/px4/px4-rs`, ours): a Rust async framework for PX4
  modules. It has `px4-sys` (bindgen FFI over uORB, WorkQueue, hrt and log),
  `px4-workqueue` (a `#[task]` whose waker is `ScheduleNow`), `px4-uorb` (typed
  `Publication<M>` / `Subscription<M>`) and `px4-msg-codegen` / `px4-msg-macros`.
- **`nros-c`**: the C API (rclc-style) that every other RTOS's C road uses, already
  built into `libnros_cpp.a` for PX4.

## Work items

### W0 -- The design question (S)

Decide, as an RFC-0039 amendment, for each language:

- **The uORB binding.** C reuses the C++ backend through its C ABI. For Rust,
  either the same, or a Rust backend over `px4-uorb`. The choice decides whether a
  Rust module links any C++ at all.
- **The module entry.** Generated C (`--lang c`) and generated Rust, or a
  hand-written `<module>_main` for C first.
- **Where the executor runs.** A `px4::ScheduledWorkItem` (C++ shim), or
  `px4-workqueue`'s `#[task]` for Rust.

The thread-stack rules of issue 2398 (`STACK_MAIN`, `CONFIG_WQ_<Q>_STACKSIZE`
against the executor floor) apply to every language.

**Acceptance:** the amendment names one road per language. Each road names the
crates, headers and the generated files it needs, and states what it shares with
the C++ road.

### W1 -- A C module: hand-written entry, then generated (M)

- A C PX4 module calls `nros_rmw_uorb_register()`, registers its topics with
  `nros_rmw_uorb_register_topic()`, and runs a C node through `nros-c`.
- `nros_px4_add_module` accepts C `SRCS` and a C `BRINGUP` entry (`--lang c`).
- One C example under `examples/px4/c/firmware`, mirroring the C++ firmware
  example.

**Acceptance:** the C example builds for `px4_sitl_default`, starts in the SITL
console, and round-trips a uORB topic with the C++ firmware module or a stock PX4
module. It has a fixture row and a cell in the `px4-sitl` lane.

### W2 -- A Rust module (M-L, after W0)

- A Rust PX4 module built as a staticlib for the PX4 target: the SITL host
  triple, and `thumbv7em-nuttx-eabihf` for a Pixhawk.
- Its `<module>_main` is exported from Rust; its executor is driven by the W0
  road.
- Its uORB topics come from the PX4 `.msg` tree through `px4-msg-codegen` or
  nano-ros's own codegen. The two must not both generate a type.
- `nros_px4_add_module` (or a sibling) links the Rust staticlib, with no cargo run
  at configure (issue 1824's rule).

**Acceptance:** one Rust example under `examples/px4/rust/firmware` builds for
`px4_sitl_default`, starts, and round-trips a uORB topic. It has a fixture row and
a cell in the `px4-sitl` lane.

### W3 -- The bridge in C and Rust (M, after W1/W2)

The C++ bridge (`examples/px4/cpp/bridge`, uORB inward and zenoh or XRCE outward)
gets a C and a Rust twin, or W0 rules that the bridge stays C++ and says why.

**Acceptance:** each twin carries one topic from uORB to a host `ros2 topic echo`
under SITL, or the ruling is recorded.

### W4 -- The book and the starter (S)

The PX4 pages drop "C++ only" for each language W1 to W3 lands. The SITL starter
gains the Rust and C tabs the three-language ruling asks for, each with its own
probe track.

**Acceptance:** `book/src/guide/platforms/px4.md` and the SITL starter show the
landed languages, each replayed by a probe track.

## Acceptance for the phase

- An in-firmware PX4 module can be written in C, C++ and Rust, each with an
  example, a `px4-sitl` cell and a probe-replayed book tab.
- Or W0 rules a language out for PX4, and the book says why.

## Out of scope

- uORB services (RFC-0011 section 7; deferred in phase-542).
- Pixhawk hardware runs (phase-542 W8.b; phase-547 is the emulator spike).
- The XRCE companion examples, which are already Rust and run beside PX4.
