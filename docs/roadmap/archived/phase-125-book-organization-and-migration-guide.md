# Phase 125 — Book organization + ROS 2 migration path

> **Archived 2026-05-18 — closed.** 9/0 checkbox ratio. Status
> self-declares "Source reorganization landed 2026-05-14.
> Navigation, ROS 2 setup comparison, migration-guide
> scaffold, link hygiene, and the first platform/debug split
> are complete." Remaining work (incremental polish of
> individual platform pages) tracked as in-flight docs work,
> not a phase-scoped item.

**Goal.** Rework the mdBook so an experienced ROS 2 user can move
from first install to a working nano-ros node without reading
implementation internals first. The book should explain what changes
relative to standard ROS 2 setup, keep platform setup discoverable,
and prepare a dedicated migration guide for rclcpp / rclc / rclrs
users.

**Status.** Source reorganization landed 2026-05-14. Navigation,
ROS 2 setup comparison, migration-guide scaffold, link hygiene, and
the first platform/debug split are complete. Remaining future work is
incremental polish of individual platform pages as those targets
change.

**Priority.** P1 — documentation gate for Phase 123's source-ship
setup and the planned migration-guide chapter.

**Depends on.** Phase 123 Stream A (source checkout inside a ROS 2
workspace, `tools/setup.sh --target=<platform>-<rmw>`, colcon-first
consumer flow). Builds on Phase 122's two-layer API terminology and
Phase 124's continued RMW ABI cleanup, but does not block them.

## Why now

The current book has the right raw material, but the ordering is
optimized for contributors rather than ROS 2 users:

- `Reference` appears before `Concepts`, so readers reach API tables
  before the execution model is explained.
- `Concepts`, `Design`, and `Internals` repeat RMW, platform, and
  architecture material at mixed depths.
- Platform setup pages live under Getting Started, making the initial
  path long and target-specific.
- ROS 2 migration context is split across
  `concepts/ros2-comparison.md`, `getting-started/ros2-interop.md`,
  `design/client-library.md`, and `design/rmw-vs-upstream.md`.
- Phase 123 changed the expected setup story: standard ROS 2 users
  should see nano-ros as a source package in `src/`, not as a
  prebuilt SDK or crates.io dependency.

The book should first answer: "I know ROS 2; what is different, what
do I run, and where do I put this in my workspace?"

## Target organization

Proposed top-level order:

1. **Introduction**
2. **Start Here for ROS 2 Users**
3. **User Guide**
4. **Platform Guides**
5. **Concepts**
6. **Porting Guide**
7. **Design Rationale**
8. **Internals**
9. **Reference**

The rule is user journey first, implementation depth later:

- **Start Here** covers setup, first node, ROS 2 interop, differences
  from standard ROS 2, and backend choice.
- **User Guide** covers operational topics: configuration, message
  generation, QoS/status/discovery behavior, serial transport, and
  troubleshooting.
- **Platform Guides** hold target-specific setup for POSIX, Zephyr,
  FreeRTOS, NuttX, ThreadX, bare-metal QEMU, ESP32, and PX4.
- **Concepts** explain mental models only: architecture, execution
  model, platform model, `no_std`/`alloc`/`std`, and RTOS cooperation.
- **Design Rationale** explains why APIs are shaped this way.
- **Internals** collects maintainer and implementation notes.
- **Reference** moves to the end as lookup material.

## Work items

- [x] **125.A — Navigation-only SUMMARY rewrite.**
  Reorder `book/src/SUMMARY.md` to the target organization without
  rewriting page bodies. Include `getting-started/px4.md`, which
  currently exists but is not listed. Move generated `book/book/`
  output out of scope for source edits.

- [x] **125.B — Link hygiene pass.**
  Fix or remove stale links found during review:
  `first-app-rust.md`, `first-app-c.md`,
  `../internals/board-crate.md`, `../internals/rmw-api.md`, and
  `../internals/porting-platform/README.md`. Prefer links to
  existing `native.md`, `porting/custom-board.md`,
  `reference/rmw-api.md`, and `porting/overview.md`.

- [x] **125.C — Add "Setup compared to standard ROS 2".**
  New early page under `Start Here for ROS 2 Users`. It should
  compare:
    - ROS 2 distro install + `rosdep` + `colcon build` +
      runtime `RMW_IMPLEMENTATION`.
    - nano-ros source checkout in workspace `src/` +
      `tools/setup.sh --target=<platform>-<rmw>` + selective
      submodules + static/local builds + compile-time RMW/platform
      selection.
  This page is the foundation for the later migration guide.

- [x] **125.D — Trim Getting Started.**
  Keep only the shortest path to a working node and ROS 2 interop.
  Move target-specific pages into Platform Guides. Rename or split
  `getting-started/native.md` so it is clearly either the first
  native Rust node or the POSIX platform page, not both.

- [x] **125.E — De-duplicate Concepts.**
  Reduce `concepts/architecture.md` to a concise map. Move deep crate
  graphs, TSN, verification, safety, and board implementation details
  to Internals or Porting. Keep feature axes in `platform-model.md`
  and make `configuration.md` link there instead of repeating the
  table.

- [x] **125.F — Clarify RMW content by audience.**
  Assign one role to each RMW page:
    - `user-guide/rmw-backends.md`: choose a backend.
    - `concepts/ros2-comparison.md`: migration orientation.
    - `design/rmw.md`: abstraction rationale.
    - `design/rmw-vs-upstream.md`: deep upstream `rmw.h`
      comparison.
    - `internals/rmw-backends.md`: host-language and registry policy.
  Remove repeated setup snippets unless they serve that page's role.

- [x] **125.G — Normalize platform guide template.**
  Each platform page should follow the same shape: when to use it,
  prerequisites, setup, build/run minimal example, RMW/API selection,
  testing, troubleshooting, and links to internals/reference. Move
  long debug appendices, such as FreeRTOS LAN9118 register material,
  into Internals.

- [x] **125.H — User guide cleanup.**
  Keep `configuration.md`, `message-generation.md`,
  `serial-transport.md`, and `troubleshooting.md` practical. Promote
  status events / QoS / discovery behavior into User Guide if users
  need it while writing applications.

- [x] **125.I — Migration guide scaffold.**
  After the organization stabilizes, add a migration guide covering:
  node lifecycle, publishers/subscriptions/services/actions,
  executor/spin differences, QoS mapping, parameters, message
  generation, backend selection, and common porting traps for
  rclcpp / rclc / rclrs users.

## Non-goals

- Do not rewrite API semantics as part of this phase.
- Do not turn design pages into tutorials; keep design rationale
  separate from user workflow.
- Do not duplicate generated API reference material in prose pages.
- Do not update generated `book/book/` artifacts until the source
  organization is settled.

## Acceptance criteria

- A ROS 2 user can follow the first section from clone to first
  interop test without visiting Internals.
- Every top-level section has a single audience and depth.
- Platform pages share a recognizable template.
- RMW material has no uncontrolled repetition; each page links to
  the deeper page instead of restating it.
- The migration guide can be written as a thin mapping chapter over
  stable setup, concepts, and user-guide pages.
