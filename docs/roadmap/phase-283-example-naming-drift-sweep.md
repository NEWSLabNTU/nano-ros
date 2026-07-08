# Phase 283 — Example naming-drift sweep

Status: **In progress — 2026-07-08** · Implements issue #136 · Informs RFC-0026
(examples layout/naming).

> **Goal.** Remove the small, non-functional naming/style drifts phase-277 left
> across the examples tree so cross-platform diffs are clean and a reviewer
> reads the same names for the same role everywhere. Source-only, no path
> changes — the directory renames (`_entry` → `-entry`) stay OWNED by phase-275
> and are an explicit non-goal here.

## Why

Phase-277 unified example *content* (chatter parity, dep shape, tree layout) but
issue #136's audit found three source-level drifts still in the tree. None are
bugs; they cost review time and make same-role cross-platform diffs noisier than
they should be. #136 collected them for one mechanical sweep instead of ad-hoc
edits — this phase is that sweep.

Already handled (not in scope): item 4 (`_entry` rename — phase-275), item 5
(duplicate 0125/0126 ids — renumbered + archived).

## Method — each wave is CHECK then FIX

Every wave first re-derives the CURRENT drift set (a `check` job — the tree
moves under active phases, so never trust the 2026-07-03 inventory blindly),
then applies the mechanical rename (`fix`), then verifies. A rename is only
"done" when the check job returns empty AND the affected examples still build /
the shape tests stay green.

## Waves

### W1 — Rust node struct: `TalkerNode` → `Talker`
The board-driven bare-metal + stm32 talkers name the struct `TalkerNode`; every
other platform and all workspace node pkgs use plain `Talker` (matches the C++
class name). Converge on `Talker`.

- [x] **W1.check** — `grep -rln 'TalkerNode' examples/qemu-arm-baremetal/rust
  examples/stm32f4/rust --include='*.rs' --include='*.toml'`. Baseline: 6
  examples (`qemu-arm-baremetal/rust/{talker,serial-talker,talker-rtic,
  talker-rtic-mixed,talker-xrce}`, `stm32f4/rust/talker`), each with 4 `lib.rs`
  refs (`struct`, `impl Node`, `impl ExecutableNode`, `nros::node!(…)`) + the
  `[package.metadata.nros.node] class = "<crate>::TalkerNode"` in Cargo.toml.
- [x] **W1.fix** — rename `TalkerNode` → `Talker` in each `lib.rs` (all 4 refs)
  AND the Cargo.toml `class` string. Both MUST stay in sync — the
  `example_shape::component_class_strings_match_package_name` lint checks the
  class ↔ crate/struct relationship, and `nros::node!` registers by class.
- [x] **W1.verify** — `example_shape` green (classification + class-string
  tests); the check job returns empty; at least one affected example compiles
  (bare-metal cross-build where the toolchain is provisioned, else the
  shape/lint tests are the gate — flag any un-cross-built example, "no silent
  caps").

### W2 — C++ namespace word order → `<plat>_cpp_<case>`
Same-role C++ examples spell the namespace three ways; the majority
(freertos/nuttx/threadx-linux/riscv64-threadx) is `<plat>_cpp_<case>`. The
Zephyr C++ set is the outlier (`nros_zephyr_<case>_cpp`); native C++ uses an
anonymous namespace. Converge the Zephyr set on `zephyr_cpp_<case>`; leave
native anonymous (documented majority for native).

- [ ] **W2.check** — `grep -rln 'namespace nros_zephyr' examples/zephyr/cpp
  --include='*.cpp' --include='*.hpp'`. Baseline: 14 files across
  talker/listener/service-{client,server}/action-{client,server} +
  cyclonedds/talker-aemv8r.
- [ ] **W2.fix** — rename each `namespace nros_zephyr_<case>_cpp` →
  `zephyr_cpp_<case>` in both the `.hpp` and `.cpp` (declaration + any
  qualified uses); keep the per-role `<case>` token. Do NOT touch
  `examples/zephyr/cpp/cyclonedds/talker-aemv8r` if it is a user-owned untracked
  file (respect worktree changes).
- [ ] **W2.verify** — check job empty; a zephyr C++ example builds (west lane
  where provisioned).

### W3 — `setvbuf` uniformity per platform
41 C/C++ example sources call `setvbuf(stdout, NULL, _IONBF/_IOLBF, …)` for
unbuffered logging (test harnesses read line-buffered output); some same-role
siblings omit it (e.g. `examples/zephyr/cpp/talker` and the native C++ set lack
it while `examples/zephyr/c/talker/src/Talker.c` has it).

- [ ] **W3.check** — per platform, list which hosted C/C++ examples have vs lack
  a `setvbuf` call; decide the per-platform rule (a hosted platform whose
  harness reads stdout needs it on EVERY example, or none does — with the
  reason).
- [ ] **W3.fix** — add (or remove) the `setvbuf` call to make each platform
  uniform, with a one-line comment stating why (harness line-buffering vs
  embedded no-op).
- [ ] **W3.verify** — check job shows uniformity per platform; affected e2es
  still observe output (no regression in a spot-checked hosted lane).

## Non-goals

- `_entry` → `-entry` directory rename (item 4) — OWNED by phase-275; RFC-0026
  blesses the underscore until that phase closes. The fixture manifest, `just`
  lanes, and docs all key on the current names; renaming ahead of 275 breaks
  them.
- Duplicate 0125/0126 issue ids (item 5) — already renumbered + archived.
- `component-poc` / `component-node-poc` / `transform-poc` dir moves — owned by
  phase-242.
- Any path/directory change — this phase is source-only.

## Acceptance

- `just format` clean; the affected examples build on provisioned toolchains
  (bare-metal for W1, west for W2), un-cross-built ones flagged.
- `example_shape` suite green (W1's class-string + classification lints).
- Each wave's check job returns empty.
- `grep -r 'namespace nros_zephyr' examples/zephyr/cpp` and
  `grep -r 'TalkerNode' examples/{qemu-arm-baremetal,stm32f4}/rust` both empty.

## Sequencing

W1, W2, W3 are independent source-only sweeps — land in any order. W1 (Rust
struct) is the smallest/cleanest and has the tightest test gate (`example_shape`
lints), so it lands first; W2 (C++ namespace) and W3 (`setvbuf`) follow.
