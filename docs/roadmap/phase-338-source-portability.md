# Phase 338 — Source portability: one body per (language, program)

**Goal:** the user-authored source of an example is **identical across every
supported target**, with exceptions declared as data rather than tolerated as
silence. Then the per-platform copies can be folded, because nothing is lost by
folding them.

**Implements:** the portability half of RFC-0026 (examples are standalone
copy-out projects — this phase makes them *the same* project).
**Related:** [phase-329](phase-329-test-taxonomy-completion.md) (the gate lands
in its guards bucket), [phase-337](phase-337-board-support-matrix.md) (which
targets we promise; this phase is what makes one source reach them),
[RFC-0064](../design/0064-board-support-organization.md) (the same
one-source-many-targets principle, applied to board crates).

**Status.** DRAFT — not started.

---

## The finding that shapes this phase

**The portable shape already exists in-tree, and it is already ceremony-free.**
The `-entry` example variant is, in full:

```rust
// src/lib.rs
#![no_std]
pub use freertos_rs_talker::register;

// src/main.rs
nros::main!();
```

No `force_link_backend!`, no `<board>_app_main!`, no `extern crate <board> as _`.
The `nros::main!()` macro and the generated entry own all of it, and it is proven
on three platforms today (freertos, nuttx, threadx-linux). Across those three the
`main.rs` bodies are identical and the `lib.rs` bodies differ **only in the
re-exported node-package name** (`freertos_rs_talker` vs `nuttx_rs_talker`) —
a naming choice, not a technical requirement.

So this phase is **a migration to a shape that already works**, not an invention.
The generator already has what it needs: `emit_rust.rs` emits both the hosted
`fn main()` and the embedded `#[unsafe(no_mangle)] extern "C" fn main()`, and it
already resolves the board through `board_path_for` (`emit_rust.rs:27-29,189`).

## Measured baseline (2026-08-04)

**C and C++ are already portable, including across the native/embedded boundary:**

| program | shared body | platforms sharing it |
|---|---|---|
| `c/talker` | 89 loc | **6 of 7** — incl. native (verified byte-identical vs nuttx) |
| `c/action-server` | 167 loc | 5 of 6 incl. native |
| `c/service-server` | 107 loc | 5 of 6 incl. native |
| `c/service-client` | 103 loc | 5 of 6 incl. native |
| `cpp/action-server` | 76 loc | 5 of 6 incl. native |
| `cpp/talker` | 62 loc | 5 of 6 incl. native |
| `cpp/service-server` | 53 loc | 5 of 6 incl. native |
| `cpp/listener` | 46 loc | 5 of 6 incl. native |

**Rust is portable within groups, and the gaps are pure ceremony.** `rust/talker`
spans 9 platforms:

| body | loc | platforms |
|---|---|---|
| RTOS group | 34 | freertos, nuttx, esp32-baremetal, threadx-linux |
| bare-metal group | 38 | qemu-arm-baremetal, stm32f4 |
| threadx-riscv64 | 37 | = RTOS body **+3 lines** |
| zephyr | 40 | = RTOS body **+6 lines** |
| native | 91 | hosted `main.rs`, a different shape entirely |

The deltas, verbatim:

```rust
// threadx-riscv64, +3
extern crate alloc;
extern crate nros_board_threadx_qemu_riscv64 as _;
nros_board_threadx_qemu_riscv64::cyclonedds_app_main!(register);

// zephyr, +6
extern crate zephyr;
#[cfg(feature = "rmw-zenoh")] nros::force_link_backend!(nros_rmw_zenoh);
#[cfg(feature = "rmw-xrce")]  nros::force_link_backend!(nros_rmw_xrce_cffi);
nros::zephyr_component_main!(Talker);
```

Every line is the staticlib-DCE pitfall already in CLAUDE.md — no POSIX-style
ctor sections on RTOS targets, so backend registration must be an explicit call
and the entry macro must name the board crate. **None of it is user logic.**

**Not every divergence is a defect.** `c/listener` native is 113 loc against the
embedded group's 97 because native carries an `NROS_SUB_TYPE` env switch letting
tests select int32 vs string. That is a deliberate test affordance and must be
*declared*, not deleted.

---

## W1 — The portability gate (FIRST, before changing any source)

Land the measurement before the fixes, so every later wave has a scoreboard and
nothing silently regresses.

- [ ] **W1.a** A guard test that, for each (language, program), normalizes every
      platform copy (strip comments and blank lines, collapse whitespace) and
      asserts the copies within a **portability group** are byte-identical.
      Source comparison only — no fixture, no boot, no QEMU. Belongs in
      phase-329's guards bucket.
- [ ] **W1.b** **Exceptions are DATA with a reason, and they do not escape the
      gate — they form their own group.** Two kinds:
      - **Shape exceptions** — Zephyr's component convention (`Talker.c`,
        `Talker.hpp`, 34 loc against C's 89). Zephyr users expect that shape;
        this is a permanent, declared exception. The gate still asserts every
        Zephyr copy is identical *to the other Zephyr copies*.
      - **Affordance exceptions** — native's `NROS_SUB_TYPE` hook. Declared with
        the test that needs it, so deleting the test deletes the exception.
      An undeclared divergence is a failure. A declared one is a row with a
      reason — the same rule `Tier::CarveOut` already follows.
- [ ] **W1.c** Record the baseline table above as the gate's starting state, so
      W2–W5 are measurable as groups merging rather than as vibes.

## W2 — Migrate the plain Rust examples onto the `-entry` shape

The ceremony moves into the generated entry, where `nros::init`, executor open,
RMW registration and the spin loop already live.

- [ ] **W2.a** Teach the generated entry to emit what the examples currently
      hand-write: `force_link_backend!` for the selected RMW(s), the board's
      `*_app_main!` / `zephyr_component_main!` invocation, and the
      `extern crate <board> as _` / `extern crate alloc` link anchors. The
      generator already knows the board (`board_path_for`), so this is emission,
      not new resolution.
- [ ] **W2.b** Migrate the plain `rust/{talker,listener,service-*,action-*}`
      examples to the two-file `-entry` shape.
- [ ] **W2.c** Normalize the node-package names so the `pub use <node>::register;`
      line is identical across platforms (today `freertos_rs_talker` vs
      `nuttx_rs_talker`). Naming rules already exist in
      `examples/workspaces/README-layout.md`; extend them to examples.
- [ ] **W2.d** Normalize the `#![no_std]` / `#![no_main]` inconsistency — the
      freertos `talker-entry/src/main.rs` carries both attributes while the nuttx
      and threadx-linux ones carry neither, for the same generated entry.
      *Result:* threadx-riscv64 and zephyr join the RTOS group;
      `rust/talker` goes from 4 bodies to 2 (RTOS+baremetal, and native).

## W3 — Close the native/embedded Rust split

C already proves this gap is closable: `c/talker` shares one 89-line body across
native *and* five embedded platforms. Rust splits a 91-line hosted `main.rs` from
a 34-line `lib.rs`.

- [ ] **W3.a** Establish why the hosted Rust example is 91 lines when its embedded
      sibling is 34 — how much is genuinely hosted-only (arg parsing, signal
      handling, `std` logging) versus ceremony the generated entry could own on
      both sides.
- [ ] **W3.b** Bring native onto the entry shape so the body is the same 34 lines,
      with the hosted/embedded difference living entirely in the generated `main`
      that `emit_rust.rs` **already** emits in both shapes.
      *Result:* `rust/talker` reaches ONE body across **group A** (below) — the
      portability claim, demonstrated rather than asserted.

**Correction, measured 2026-08-04 — there are THREE groups, not one plus Zephyr.**
An earlier draft of this phase claimed all 9 platforms converge. Diffing
bare-metal against the RTOS body shows that is wrong, and the difference is not
ceremony:

```rust
// qemu-arm-baremetal, vs the RTOS body
const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
static LOGGER: Logger = Logger::new("talker");   // nros_log, not `log`
```

Bare-metal has **no RTOS scheduler**, so it uses deferred dispatch plus an
explicit tick loop, and it cannot use the `log` facade. That is a real execution
model, not a portability defect. So the target is:

| group | platforms | why it is its own group |
|---|---|---|
| **A — scheduled** | native, threadx-linux, freertos, nuttx (arm+riscv), threadx-riscv64 | an RTOS/OS scheduler runs callbacks |
| **B — bare-metal** | qemu-arm-baremetal, qemu-esp32-baremetal | deferred dispatch + explicit `tick()`, `nros_log` facade |
| **C — Zephyr** | zephyr | component authoring shape (`Talker.c`/`.hpp`) |

Part of B's delta *is* reducible — the `log` vs `nros_log` split is a facade
problem, and `DISPATCH`/`tick` could carry defaults. W3.c measures how much;
whatever survives is a declared group, not a defect.

- [ ] **W3.c** Measure B's irreducible delta after unifying the logging facade and
      defaulting `DISPATCH`/`tick`. Record the remainder as group B's declared
      reason. **Do not force B into A** — an execution-model difference expressed
      as one body with cfg branches is worse than two honest bodies.

## W4 — Arch portability: the FreeRTOS Cortex-M4F/M7 panic

**Owner moved here from phase-337 W1.a** — it is a source-portability defect, so
it belongs with the rest of them. phase-337 keeps a pointer.

- [ ] **W4.a** `packages/boards/nros-board-freertos/build.rs:273-287` **hard-panics**
      for any `thumb*` target that is not `thumbv7m` unless `FREERTOS_CFLAGS` is
      set, even though `config/freertos-lwip/nros-platform.toml` now lists
      `arch = ["cortex-m3", "cortex-m7"]`. Derive the cflags from the `[arch.*]`
      profile the platform config already carries; keep `FREERTOS_CFLAGS` as the
      rung-1 override. Every industrial FreeRTOS board is M4F/M7 (S32K344 is M7),
      so this is the difference between the matrix being reachable and being a
      claim.
      *Verify:* a compile-only `thumbv7em-none-eabihf` check in the embedded lane.

## W5 — Board-shaped branches that should be platform- or capability-shaped

Two places where a **board name** reaches a decision that is not about that board.
Both were found while auditing the net seam (RFC-0064 R3), which is otherwise
clean — one `net.c` per platform, per-board deltas expressed as weak symbols and
macro arguments.

- [x] **W5.a — DONE 2026-08-04.** `cmake/NanoRosFeatureSet.cmake` matched
      `_FS_BOARD STREQUAL "threadx-linux"` vs `"riscv64-qemu"` to pick the std vs
      `alloc`+`panic-halt` libc tier, and `FATAL_ERROR`d otherwise — so a third
      ThreadX board could not exist without editing this file.

      The tier turned out to need no new capability: it is exactly `_cross`,
      which the function **already computes**. threadx-linux is a host build;
      threadx-qemu-riscv64 is a cross build (its `[board.cmake] toolchain_file`
      sets `CMAKE_SYSTEM_NAME`). Deriving from `_cross` generalizes to any future
      ThreadX board with no edit here, and `BOARD` stops being load-bearing.
      Verified with a standalone `cmake -P` harness: both live boards produce
      **byte-identical** feature lists to the old branch, and a third board now
      resolves instead of fatalling.

- [x] **W5.b — MEASURED 2026-08-04; no untangling needed, and the earlier
      "blocks phase-337 W7.b" claim was too strong.** `orin-spe` does reach
      link-feature selection by name (`nros-zpico-build/src/runner.rs:112,240,
      344-345,434-435,543-544` + `LinkPolicy::orin_spe()` +
      `config/orin-spe/nros-platform.toml` + the `zpico-sys` `orin-spe` feature),
      which is the shape RFC-0064 flagged. But the chain is **self-contained**:
      the only crate that enables the `orin-spe` feature is
      `nros-board-orin-spe` itself (`its Cargo.toml:81`), and no example, no
      fixture and no test references it. The board crate's own comment already
      says "orin-spe is a FreeRTOS board, not a platform" (phase 121.10).

      So deleting the board crate makes the whole chain dead code rather than
      breaking anything. phase-337 W7.b is an **ordering**, not a dependency:
      delete the crate, then in the same change delete the now-dead
      `config/orin-spe/`, `LinkPolicy::orin_spe()`, the `CARGO_FEATURE_ORIN_SPE`
      branches, the `zpico-sys` feature, the `nros-sdk-index.toml [board.orin-spe]`
      entry and the `zpico_backend` lint value. Recorded here so W7.b does not
      re-derive it.

## W6 — Fold the per-platform copies (gated on W1–W3 green)

Only once the gate proves the bodies are identical. The point of the earlier
waves is that by here, folding destroys nothing.

- [ ] **W6.a** Fold the copies within each portability group to one canonical
      source plus per-platform build configuration (`Cargo.toml`,
      `.cargo/config.toml`, `package.xml`, `CMakeLists.txt`), which is where the
      real platform difference has always lived.
- [ ] **W6.b** **Preserve what the copies are for.** They exist so a user can copy
      one out (RFC-0026) and so `talker`/`listener` mirror the ROS 2 examples that
      make nano-ros legible to ROS users. Folding must keep a per-platform copy
      *materializable* — generated or templated — not force users into a
      workspace. If that cannot be kept, do not fold; the gate alone already
      removes the drift risk, which was the actual problem.
- [ ] **W6.c** **Zephyr stays an exception**, declared with its reason: its
      component shape (`Talker.c` / `.hpp`) is the convention Zephyr users expect,
      not a portability failure. Same for any other shape exception W1.b records.

---

## Sequencing

```
W1 (gate)  ──►  W2 (rust ceremony)  ──►  W3 (native split)  ──►  W6 (fold)
W4 (arch panic)   ─ independent, small, unblocks phase-337's industrial claim
W5 (board branches) ─ independent; W5.b BLOCKS phase-337 W7.b
```

W1 first is the load-bearing choice: it is cheap, it stands alone, and without it
every later wave is unverifiable and every fold is a leap.

## Acceptance

- [ ] The gate exists and passes, with every exception carrying a written reason.
- [ ] `rust/talker` and `rust/listener` are ONE body **per group** — A, B and C —
      where today group A alone is split across 4 bodies plus 2 near-misses.
      Three groups is the honest target; one is not.
- [ ] No example source contains `force_link_backend!`, `*_app_main!` or
      `extern crate <board> as _` — the generated entry owns all of it.
- [ ] A `thumbv7em-none-eabihf` FreeRTOS build does not panic.
- [ ] No cmake or build-script decision branches on a board *name* where a
      capability or platform is what it means.
- [ ] The book can state "the same source runs on every supported target" with the
      gate as its citation.

## Risks

- **Folding too early.** W6 before W1–W3 would delete copies that are not actually
  identical yet. The gate is the precondition, not the paperwork.
- **The `-entry` shape may not cover every example.** It is proven for
  talker/listener/service/action on three platforms; RTIC, embassy, serial and
  uORB variants have their own entry shapes and may stay exceptions. Measure
  before promising — RFC-0064's exploration log is explicit that inferred gaps do
  not survive contact.
- **W5.b is on phase-337's critical path.** `orin-spe` looks like a deletable
  scaffold and is not. Untangle before anyone deletes it.
- **This phase buys no CI time either.** It removes source copies and drift, not
  fixture rows. The honest pitch is maintainability plus a portability claim that
  is finally checkable.
