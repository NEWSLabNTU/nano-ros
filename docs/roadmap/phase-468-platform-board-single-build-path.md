# Phase 468 — one build path for every platform and board

**Status (2026-09-25). Opened from a review of all 14 platform and 22 board
entries. **W1 has landed** (2026-09-25) — read its section, not this line, for
what it measured. The review's first finding was that the system
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
| descriptors in the TREE | **7** — the five above plus `config/{bare-metal,generic}`; W1 made it 7 answering 11 names |
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

**LANDED 2026-09-25.** All four boxes below are ticked with the measurement that
closed each. The rest of this section is kept as written and annotated in place,
because two of its numbers moved.

Today it was not. The loader stated the current rule outright:

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

**Two numbers in the paragraph above moved, and a reader needs to know which.**
Of phase-400 W6's three casualties, `zephyr` and `threadx-linux` were already
answered before W1 started (`threadx` grew its two aliases in issue 1145);
`esp32` was the last, and the only one left in
`check-platform-name-answered`'s baseline. And the population was never three:
it is **8 names, declared by 14 `[[board]]` entries** — `bare-metal`, `esp32`,
`freertos`, `nuttx`, `posix`, `threadx-linux`, `threadx-riscv64`, `zephyr`. The
work item's "the enumeration decides whether there are others" answered *no
others*, which is a smaller job than this section expected.

- [x] **Enumerate every platform NAME that any board, fixture or lane resolves,
      and which package directory each keys to.**
      The population is board-declared `platform = "…"`, which is what
      `nros ws board-facts` emits as `NROS_PLATFORM_NAME` — **not** a fixture
      row's `platform`, which is a coordinate label and was the wrong
      population until issue 1362 corrected it. 8 names over 14 `[[board]]`
      entries; 7 descriptors answer 11 names between them. The keying is the
      point of the work item and the measurement confirms it: not one name
      equals its descriptor's directory in all cases — `esp32` and `bare-metal`
      are both answered by `config/bare-metal/`, `threadx-linux` and
      `threadx-riscv64` by `packages/platform/nros-platform-threadx/`.
- [x] **Every such name has a descriptor, or a descriptor that DECLARES it has
      no rungs.**
      One name was unanswered: `esp32`. The answer is **not** an explicit empty
      and not a new descriptor — `config/bare-metal/nros-platform.toml` now
      lists it in `names`, because four measurements say this platform IS
      bare-metal and the tree already acted on that everywhere except here:
      (1) `PlatformKind::Esp32::platform_feature()` already returns
      `platform-bare-metal`, in a doc-comment that says "There is no
      `config/esp32/`"; (2) `nros-board-esp32-qemu` takes
      `nros-rmw-zenoh/platform-bare-metal` -> `zpico-sys/bare-metal` ->
      `CARGO_FEATURE_BARE_METAL`, so `nros-zpico-build`'s own `platform_name`
      derivation already resolves *this file* for the vendored zenoh-pico C
      build of every esp32 image; (3) `[arch.riscv32imc]` in that file was
      written for the ESP32-C3 and says so; (4) the file declares no
      `[knobs.*]` and no `[capabilities]`, so the rungs esp32 resolves are
      byte-identical to the builtins it was falling through to — the image does
      not move, the two roads stop naming two descriptors.
      **This is the bare-metal ESP32-C3 QEMU path, not the ESP-IDF port W2
      retires** (W2 asks for that sentence in those words). W2's warning was
      right and the gate's own guess — "`esp32` has an RTOS (ESP-IDF's
      FreeRTOS) and so probably wants one" — was wrong; that text is gone with
      the baseline.
- [x] **A missing descriptor for a resolved name is a hard error naming the
      platform, the directory it looked in, and the remedy.**
      `BuildRungs::or_builtin_rungs` is `BuildRungs::require_rungs`: the
      `cargo:warning` + `T::default()` arm is gone and `UnknownPlatform` panics.
      The message is a free function with its own unit tests, because the part
      that went wrong last time was the TEXT — phase-400 W6 printed
      `…/packages/platform/threadx-linux/nros-platform.toml`, a path that could
      never exist. It now names every root searched (`BuildRungs` keeps the
      search path rather than re-deriving it, since `UnknownPlatform` carries
      only the FIRST root), the 11 names the tree does answer, that `names` is
      what answers a name, both remedies, and the buildless gate. Measured
      end-to-end: `NROS_PLATFORM_NAME=wumpus cargo build -p nros-params` exits
      101 with that message; all 8 real names build with **0**
      `cargo:warning` lines.
      Reach checked (issue 0196): the zpico road already panicked on
      `UnknownPlatform`, and `nros config explain` was already strict. One road
      was not — `arch_flags::cflags_for_target` returned `Ok(None)` for an
      unknown platform, indistinguishable from "declares no profile admitting
      this triple", which sends the reader to add an `[arch.*]` block to a file
      that is not there. It also read ONE root (issue 1486's defect a third
      time), so `config/bare-metal`'s four `[arch.*]` profiles were invisible to
      it; `config_root()` is `platform_search_path()` now and both directions
      are tested.
- [x] **A gate asserts the set, and fails on a deliberately removed
      descriptor.**
      `check-platform-name-answered` has **no baseline at all** — the ratchet
      and `BASELINE_UNANSWERED` are deleted, so an unanswered name cannot be
      silenced, only answered. Its self-test gained a control that mirrors the
      REAL descriptors into a temp root MINUS one and asserts the gate reports
      exactly the names that went with it (asserted on the DELTA, so an
      already-red tree does not make the control fail for a reason that is not
      about the control). Both directions measured by hand as well: moving
      `config/bare-metal/nros-platform.toml` aside makes the gate report
      `bare-metal` and `esp32`; renaming one board's `platform` to an invented
      name makes it report that name.

Also filed on the way through: **issue 1494** — `BuildRungs` reads every
descriptor on the search path and watches none of them, so this newly-fatal
verdict is not re-evaluated when its input moves. Measured here: adding `esp32`
to `names` did not clear the refusal until `build.rs` was touched.

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
a `ci` recipe nothing invokes — phase-451's class at platform scale,
where the dead declaration is a whole port rather than a cmake module.

**It is NOT the esp32 QEMU path, and the two are easy to conflate.**
`nros-board-esp32-qemu` declares `platform = "esp32"` and
`nros-platform-esp32-qemu` is bare-metal ("ESP32-C3 QEMU bare-metal"). Removing
the ESP-IDF port does not touch esp32 fixtures. Any change here must state
which of the two it is affecting, in those words.

- [x] The removal is measured before it is made: every referrer of
      `nros-platform-esp-idf` enumerated UNTRUNCATED, and each classified as
      port / tooling / documentation. (PR #1279.)
- [x] The port, `just/esp_idf.just` and `scripts/esp_idf/` go together — a
      recipe module for a deleted port is the same defect one level up.
      **It was that defect, briefly**: steps 2-3 deleted `just/esp_idf.just`
      and left `run esp_idf` in the `doctor`/`setup` `all` tier, so
      `just doctor tier=all` failed with `error: justfile does not contain
      recipe \`esp_idf\`` — word for word the `rmw_zenoh` failure the comment
      six lines above it already records. Step 5 removed it.
- [x] Book and `nros-sdk-index.toml` coverage follows; ESP-IDF stops being an
      answer `nros setup` or the book offers. (PR #1273.)
- [x] esp32 QEMU fixtures still build, asserted rather than assumed. Plus the
      cmake vocabulary asserted BOTH directions: `-DNANO_ROS_PLATFORM=posix`
      still configures, `-DNANO_ROS_PLATFORM=esp_idf` is now REJECTED.

Steps 6-8 remain: retype (do not delete) the zenoh vocabulary keys, the
`NROS_ESP_IDF_*` environment knobs and `.env.example`, and close issue 1282.

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
readers disagree: Kconfig via `$DOTCONFIG` on Zephyr, env elsewhere.

> **The "exactly ONE build script" this paragraph used to claim is REFUTED**
> (issue 1490, measured). FOUR call `nros_zephyr_build::knob*` —
> `nros`, `nros-platform`, `nros-rmw-zenoh`, `rmw/cffi` — and three more
> (`nros-node`, `nros-params`, `nros-rmw-xrce-cffi`) reach `$DOTCONFIG`
> through the DERIVED spelling, building the Kconfig name from the env name.
> That is what the first box meant by not assuming the claim.

Issue 0460 is the live hazard — a Kconfig knob reaching the Zephyr C lane and
not the Rust one, so an image compiles crate defaults while Kconfig says
otherwise, and where the halves disagree it is also an 0135 ABI split. It is
HELD by `check-kconfig-knob-forwarding` rather than removed.

**Scoped separately on purpose.** W1–W3 are finishable in a sitting each; this
is not, and folding it in would make the phase unfinishable — the failure mode
CLAUDE.md records for the old single `just ci`, "an instruction nobody could
afford per task, so it got followed selectively, which is worse than a smaller
instruction followed honestly".

- [x] The population first: every knob, its producer, and every reader, with
      the lane each reader runs in. The claim "one reader" is not assumed.
      **Done, and it found a live 0460 (issue 1490, PR #1281).** 52 knobs are
      forwarded by `zephyr/cmake/nros_cargo_build.cmake`. Readers split into
      TWO shapes and the shapes are not interchangeable:

      * **DERIVED** (`nros-node`, `nros-params`, `nros-rmw-xrce-cffi`) — build
        the Kconfig name as `CONFIG_{env_name}`. A knob such a reader names is
        a knob it resolves, so "does the file mention it" is a sound test.
      * **TABULATING** (`nros-zpico-build/runner.rs`, `nros-rmw-zenoh`) — an
        AUTHORED `KCONFIG_KNOBS` table, needed because their env names and
        Kconfig names are different words (`ZPICO_SUBSCRIBER_RING_DEPTH` <->
        `CONFIG_NROS_SUBSCRIBER_RING_DEPTH`). Here a mention proves nothing.

      Seven knobs were mentioned by a tabulating reader with no row. Four are
      cmake-DERIVED facts with no Kconfig symbol (legitimately table-less —
      there is no `$DOTCONFIG` rung for a number cmake computed). The other
      three were live splits, measured end to end on
      `examples/zephyr/rust/talker`: `CONFIG_NROS_SUBSCRIBER_RING_DEPTH=7`
      reached the build's `.config` while the Rust half compiled `4`.

      Two things that generalise past this phase:

      * **The BASELINE could not have shown it.** Unset, the Kconfig default
        and the crate default are both `4`, so "delivered" and "fell back to
        the same number" are ONE observation. Only a non-default probe value
        separates them — which is the method any later box here should use.
      * `check-kconfig-knob-forwarding` was GREEN over all three. It is issue
        0751's finding one arm over: 0751 hardened the DERIVED arm against
        exactly this ("the name APPEARING is not the name being resolved") and
        the tabulating arm kept the mention test. The gate asks for a ROW now.
- [ ] One resolution function, with the Kconfig and env sources as INPUTS to it
      rather than as separate call paths.
- [ ] `check-kconfig-knob-forwarding` either becomes unnecessary or narrows to
      what the new reader cannot express — stated either way.

## Acceptance for the phase

* **MET (W1, 2026-09-25).** No platform resolves to a silently empty rung set:
  every resolved name has a descriptor or an explicit declaration that it has
  none. Measured through the live path, not off the files: all 8 board-declared
  names build with 0 `cargo:warning` fall-through lines, and an unanswered name
  is a panic rather than a warning.
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
