# Phase 468 — one build path for every platform and board

**Status (2026-09-25). Opened from a review of all 14 platform and 22 board
entries. Nothing has landed. The review's first finding was that the system
this phase asks for mostly EXISTS — RFC-0049's knob ladder and RFC-0064 R5 D4's
"a board states its facts once" — so three of the four work items are about
closing its last asymmetries rather than building it. W4 is the exception and
is the largest thing here.**

## Why this phase exists

Asked whether platform and board packages follow the same build path, the
answer measured out as "boards largely do, platforms unevenly, and knob
READING not at all". Three separate designs already point the same way:

* **RFC-0049 / phase-290** — `nros-platform.toml` per platform package, with
  `[capabilities]`, `[knobs.*]`, `[build.zenoh]` and `[arch.*]`, resolved on a
  stated ladder: *builtin < platform (inherits chain) < board < env*. Loader:
  `packages/tooling/nros-platform-config/src/platform_config.rs`.
* **RFC-0064 R5 D4 / phase-375 W7** — a board states its facts ONCE, in
  `nros-board.toml`; cmake reads a mechanical projection
  (`nros board cmake-vars`). Two older faces were retired: a `board.cmake`
  sidecar carrying 14 `NROS_BOARD_*` variables, and a
  `[package.metadata.nros.board]` mirror in `Cargo.toml`.
* **`nros-board-common`** — the shared build-script crate. 8 of the 11 board
  `build.rs` route through it, organised per RTOS family (`freertos_build`,
  `nuttx_platform_build`, `threadx_sources`) over shared `arch_flags` /
  `base_config` / `policy` / `host_probe`.

What is missing is not a design. It is that the system's own absences are
silent, one port sits outside it entirely, and the knob *ladder* stops at the
point where a knob is actually read.

## What the review measured

| | |
| --- | --- |
| platform packages | 14 — 6 pure-C ports, 8 Rust crates |
| of those, carrying `nros-platform.toml` | 5 (freertos, nuttx, posix, threadx, zephyr) |
| board entries | 22 — 19 Rust crates, 2 announcement-only packages, 1 PAC |
| board `build.rs` routed through `nros-board-common` | 8 of 11 |
| build scripts calling `nros_zephyr_build::knob_usize` | **1** |

The three board `build.rs` that bypass `nros-board-common` —
`mps2-an385-pac`, `nros-board-mps2-an385`, `nros-board-threadx-qemu-riscv64` —
only emit linker scripts. That is a different job, not drift, and this phase
leaves them alone.

`packages/boards/{linux,zephyr}/` are descriptor-only ament packages carrying
`<nano_ros_provides kind="board" name="native"/>`. That is the provider-scan
announcement role (RFC-0071 D5), not a second descriptor location.

## W1 — a platform with no descriptor is an ERROR, and says so

Today it is not. The loader states the current rule outright:

> A platform with NO `nros-platform.toml` has no rungs, and that is a normal
> state — not an error.

That is a **silent default**, and it is the shape this repository has decided
against everywhere else it has come up: the sizing descriptor gives every field
a value or a `[<section>.refused]` reason; `check-c-array-pool-floors` refuses
an unruled knob rather than defaulting it. A platform that silently has no
rungs cannot be told apart from one whose rungs were lost.

The obvious version of this change has already been tried and reverted, and the
work item exists to not repeat it. phase-400 W6 turned an absent descriptor
into a `panic!` and every image on `threadx-linux`, `esp32` and `zephyr` died
in a build script:

```text
NROS_PLATFORM_NAME=threadx-linux: unknown platform `threadx-linux`:
  no …/packages/platform/threadx-linux/nros-platform.toml
```

So "make it an error" is only safe once every platform that resolves has
something to resolve TO. Note the message names a directory that does not
exist: the platform NAME and the package DIRECTORY are keyed differently, and
W1 has to establish which platform names must resolve before it can make a
missing one fatal.

- [ ] Enumerate every platform NAME that any board, fixture or lane resolves,
      and which package directory each keys to. The three names in phase-400's
      regression are the known-hard cases; the enumeration decides whether
      there are others.
- [ ] Every such name has a descriptor, or a descriptor that DECLARES it has no
      rungs — an explicit empty, not an absent file.
- [ ] A missing descriptor for a resolved name is a hard error naming the
      platform, the directory it looked in, and the remedy.
- [ ] A gate asserts the set, and fails on a deliberately removed descriptor.

## W2 — `nros-platform-esp-idf` is out, and goes

Decided: esp-idf does not join the unification. Measured, it is also not
carrying its weight today.

| | |
| --- | --- |
| `nros-platform.toml` | none — the only one of six C ports without |
| `package.xml` | none — the only one of six C ports without |
| fixtures naming it | **0** |
| CI workflows building it | **0** |
| `just/esp_idf.just` | a full module: `doctor`, `build-c-port`, `build-examples`, `ci` |

So it is a platform port that no lane builds and no fixture targets, with a
`just esp_idf ci` recipe nothing invokes — phase-451's class at platform scale,
where the dead declaration is a whole port rather than a cmake module.

**It is NOT the esp32 QEMU path, and the two are easy to conflate.**
`nros-board-esp32-qemu` declares `platform = "esp32"` and
`nros-platform-esp32-qemu` is bare-metal ("ESP32-C3 QEMU bare-metal"). Removing
the ESP-IDF port does not touch esp32 fixtures. Any change here must state
which of the two it is affecting, in those words.

- [ ] The removal is measured before it is made: every referrer of
      `nros-platform-esp-idf` enumerated UNTRUNCATED, and each classified as
      port / tooling / documentation.
- [ ] The port, `just/esp_idf.just` and `scripts/esp_idf/` go together — a
      recipe module for a deleted port is the same defect one level up.
- [ ] Book and `nros-sdk-index.toml` coverage follows; ESP-IDF stops being an
      answer `nros setup` or the book offers.
- [ ] esp32 QEMU fixtures still build, asserted rather than assumed.

## W3 — the board build wiring, confirmed and held

Boards have largely converged already, so this item is mostly about keeping
that true rather than making it true.

- [ ] A gate: a `packages/boards/*/build.rs` that compiles C or resolves a
      vendored source tree routes through `nros-board-common`, or states why
      not. The linker-script three are the exemptions and each carries a
      reason.
- [ ] The per-family modules are reachable from where they claim to be
      (`policy` is reached only by `nros-zpico-build`, `threadx_config` only
      through a `pub use` re-export — both legitimate, both invisible to a
      `nros_board_common::<mod>` grep, which is how a review nearly reported
      them dead).

## W4 — knob READING, the one that is not mostly done

The ladder resolves a knob's VALUE. It does not decide who reads it, and the
readers disagree: Kconfig via `$DOTCONFIG` on Zephyr, env elsewhere, and
`nros_zephyr_build::knob_usize` appears in exactly ONE build script in the
tree.

Issue 0460 is the live hazard — a Kconfig knob reaching the Zephyr C lane and
not the Rust one, so an image compiles crate defaults while Kconfig says
otherwise, and where the halves disagree it is also an 0135 ABI split. It is
HELD by `check-kconfig-knob-forwarding` rather than removed.

**Scoped separately on purpose.** W1–W3 are finishable in a sitting each; this
is not, and folding it in would make the phase unfinishable — the failure mode
CLAUDE.md records for the old single `just ci`, "an instruction nobody could
afford per task, so it got followed selectively, which is worse than a smaller
instruction followed honestly".

- [ ] The population first: every knob, its producer, and every reader, with
      the lane each reader runs in. The claim "one reader" is not assumed.
- [ ] One resolution function, with the Kconfig and env sources as INPUTS to it
      rather than as separate call paths.
- [ ] `check-kconfig-knob-forwarding` either becomes unnecessary or narrows to
      what the new reader cannot express — stated either way.

## Acceptance for the phase

* No platform resolves to a silently empty rung set: every resolved name has a
  descriptor or an explicit declaration that it has none.
* `nros-platform-esp-idf` and its tooling are gone, and esp32 QEMU still builds.
* Every board `build.rs` that compiles C routes through `nros-board-common` or
  states why not.
* A knob has one reader, or the exceptions are named.

## Non-goals

* The linker-script board crates (`mps2-an385-pac`, `nros-board-mps2-an385`,
  `nros-board-threadx-qemu-riscv64`). Emitting a `memory.x` is a different job
  from compiling a vendored RTOS.
* The board announcement packages (`packages/boards/{linux,zephyr}/`). They are
  the provider scan's surface, not a rival descriptor location.
* Anything about what a knob's VALUE should be — RFC-0049 owns the ladder, and
  this phase only asks who reads the result.
