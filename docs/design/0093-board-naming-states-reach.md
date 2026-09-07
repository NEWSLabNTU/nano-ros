---
rfc: 0093
title: "A board name states the reach of its build"
status: Draft
since: 2026-09
last-reviewed: 2026-09
implements-tracked-by: [phase-437]
supersedes: []
superseded-by: null
---

# RFC-0093 — A board name states the reach of its build

> **Goal.** A board name is a claim about where its artifact runs, and the
> claim is true. Name the narrowest thing the build is true for; put the stack
> last; never name the emulator; borrow the vendor's word; and leave out what
> the index already carries as a field.

**Amends nothing.** It settles the question phase-422 W7 recorded and deferred:
of two board vocabularies that both resolve, which is canonical. The answer is
neither as a house style — the rule below SELECTS a name per board, and it
selects from both vocabularies.

## 1. The problem, measured

`board=` in a `package.xml` export and `[board.*]` in `nros-sdk-index.toml` are
two namespaces for one concept. Phase-422 W7 closed the resolution half
additively: every exported board is an index key now, so `nros setup <board>`
works for five of five instead of one. It left four `[board.*]` entries
duplicating a counterpart under the other spelling, marked `# = [board.X]` and
held byte-identical by `check-board-vocabulary`.

That is stable and it is not free. Comparing every `[board.*]` body, **four
pairs are byte-identical** and the gate asserts three of them — it keys on the
`# =` marker, and `qemu-arm-baremetal` / `mps2-an385` carries none. Two names,
one build, nothing holding them in step. (`native` / `posix` are also identical
and correctly unmarked: they answer ROLE vs REACH and
`check-host-platform-vocabulary` owns that distinction.)

The deeper cost is that the names are not TRUE, in both directions:

| name | claims | measured |
| --- | --- | --- |
| `qemu-arm-freertos` | any ARM under QEMU with FreeRTOS | links `mps2_an385.ld`; one part |
| `riscv64-qemu` | riscv64 under QEMU | one machine: UART `0x10000000`, CLINT `0x02000000`, virtio-MMIO `0x10001000`, and a write to QEMU's `test-finisher` at `0x100000` — a device no silicon has |
| `nuttx-qemu-arm` | NuttX on ARM under QEMU | `CONFIG_ARCH_BOARD="qemu-armv7a"`, `CONFIG_ARCH_CHIP="qemu"`, PL011 at `0x9000000` IRQ 33, RAM at `0x40200000`; one machine |

A name that over-promises reach is not a cosmetic problem. It is what a user
reads when deciding whether their hardware is supported, and it is what a
contributor reads when deciding whether a new part needs a new board.

The other direction has a witness too: **`s32z270-freertos` is real NXP silicon
(Cortex-R52, `s32z270_rtu.ld`), and it is not an index key at all.** An
emulator-first namespace never had a name to give it.

## 2. The rule

### R1 — Name the narrowest thing the build is true for

Three reaches exist here, and every board is exactly one:

* **system** — no hardware assumption. `freertos-posix` sets no
  `CMAKE_C_COMPILER` and no linker script; `threadx-linux` runs as a userspace
  process. These travel across machines.
* **part** — one silicon part. `mps2-an385` (`mps2-an385.x`, `mps2-an385-pac`),
  `mps3-an536` (`an536.ld`), `s32z270` (`s32z270_rtu.ld`), `esp32-c3`.
* **machine model** — one emulated or modelled machine. QEMU `virt`, `rv-virt`,
  Arm's FVP models. Pinned by a memory map, peripheral addresses and a boot
  contract, exactly like a part.

Part and machine are the same reach — one target — and take the same shape.
Never name something broader than what runs; never narrower.

### R2 — `<where>-<stack>`, stack always last

The head answers *where does this run*, the suffix *which stack runs there*.
Both are needed because one target hosts several stacks: `mps2-an385` carries
bare-metal, FreeRTOS and Zephyr. A fixed position makes a family sort together
and makes the missing member of a family visible.

### R3 — Never name the emulator

QEMU is how we RUN a machine, not what the build targets. The tree already
proves the point: `cmake/board/nano-ros-board-esp32-c3.cmake` is *"board overlay
for ESP32-C3 (real silicon) + ESP32-C3 QEMU"* — one name, both, because the name
is the part.

Where the target exists only under an emulator, name the MACHINE — `virt`,
`rv-virt`, `armv7a` — not the emulator that hosts it. This is what makes the
namespace able to hold real silicon and a simulator side by side, which
`s32z270-freertos` needs it to do.

Nothing in the tree keys on the literal string `qemu`: the test harness resolves
emulators by TOOL (`qemu_system_riscv32_path()`), never by parsing a board name.
So the word carries no meaning a program depends on.

### R4 — Prefer the vendor's own name, and R4 outranks R5

`CONFIG_ARCH_BOARD="qemu-armv7a"`, `CONFIG_ARCH_BOARD="rv-virt"`, Arm's
`mps2-an385`, NXP's `s32z270`. A borrowed name is CHECKABLE against upstream;
an invented one is a second vocabulary of its own, which is the thing this RFC
exists to stop.

Where upstream has no name, spell the machine. ThreadX has no board layer at
all — every address in `nros-board-threadx-qemu-riscv64` is a literal in our own
C — so `virt` comes from QEMU's machine list rather than from a vendor BSP.

### R5 — Leave out what the index already carries

`arch` and `platform` are FIELDS on every `[board.*]` entry. Repeating them in
the name is the redundancy that let one build wear two names without anything
noticing: `qemu-arm-freertos` and `mps2-an385-freertos` differ in no field.

Exception: a **system** board, where the ABI is the constraint rather than an
attribute of a target — `freertos-posix` is the whole claim.

## 2b. The fixture-platform axis is a FAMILY axis (added 2026-09-08)

Board names are not the only board-ish vocabulary. A survey found **five**:

| # | vocabulary | shape |
| --- | --- | --- |
| 1 | index `[board.*]` key | this RFC's subject |
| 2 | index `.platform` field | `bare-metal freertos nuttx posix threadx zephyr` |
| 3 | `examples/fixtures.toml` `platform =` | a lane COORDINATE, keyed by `PlatformId` |
| 4 | `just` scope | `native … threadx_linux threadx_riscv64 esp32 …` (underscores) |
| 5 | `just` module filename | `qemu-baremetal threadx-linux …` (hyphens) |

Vocabulary 3 is a different AXIS from 1, and the difference is the point. Ten of
its twelve values name a platform FAMILY — `linux`, `zephyr`,
`zephyr-cortex-m`, `threadx-linux`, `threadx-riscv64`, `freertos`,
`freertos-posix`, `nuttx`, `nuttx-riscv`, `esp32`. Two name a BOARD:
`qemu-arm-baremetal` (21 rows) and `qemu-esp32-baremetal` (3).

**R6 — a fixture platform names a family, never a board.** Renaming the two
odd entries to their new BOARD names would leave the axis just as mixed, spelled
differently. They take family names instead:

    qemu-arm-baremetal    -> baremetal      (the only bare-metal family; the word is free)
    qemu-esp32-baremetal  -> esp32          (COLLAPSES a duplicate — see below)

The esp32 case is the evidence. `matrix.rs` already maps ONE `PlatformId` to
TWO tokens:

    PlatformId::Esp32Qemu => &["esp32", "qemu-esp32-baremetal"],

`esp32` carries the workspace fixture; `qemu-esp32-baremetal` carries two
example leaves and a test bin. One platform, two coordinates, no stated reason —
the same duplication §1 found in the index, one namespace over. Collapsing to
`esp32` REMOVES it; renaming to `esp32-c3-baremetal` would add a third spelling.

Vocabularies 4 and 5 are a separate defect — `threadx_linux` and
`threadx-linux` are one concept with two separators — and a separate rename with
its own blast radius. Out of scope here, deliberately: bundling it would put two
unrelated decisions in one sweep.

## 3. What the rule is not

**Not "hardware-first" or "emulator-first".** Both framings pick a word order
and then have to argue about it. R1 asks what the build is true for and the name
follows; the two vocabularies each turn out to be right for some boards and
wrong for others.

**Not a claim that the index vocabulary was careless.** It is uniformly
`qemu-<arch>-<rtos>` and reads consistently. It is also already mixed —
`fvp-aemv8r-smp` is machine-first and sits among the `qemu-*` keys — so
"consistency" was not available to defend either.

**Not about `deploy=` — with one exception, measured.** A deploy names a FAMILY
(`threadx`, `nuttx`), and the `board=` beside it selects the implementation.
RFC-0087 D3 and `check-board-alias-unique` own that; this RFC does not touch it.

The exception: a deploy token that IS ALSO a board key follows its board.

    git grep -hoP '^\s*deploy\s*=\s*"\K[^"]+' -- '*.toml' | sort | uniq -c
      2 qemu-esp32-baremetal

`qemu-esp32-baremetal` is an index board key, a fixture platform, a member of
`NROS_FIXTURE_SHARED_PLATFORMS`, and a `deploy=` value in two manifests. Leaving
the deploy token behind would not preserve a separate vocabulary — it would
leave a THIRD spelling of one thing, which is the defect this RFC exists to
close. So it moves with the board: `deploy = "esp32-c3-baremetal"`.

Deploy values that are not board keys — `native`, `freertos`, `nuttx`,
`threadx-linux`, `zephyr`, `qemu-mps2-an385`, `rtic-mps2-an385`,
`threadx-qemu-riscv64`, `esp32-qemu` — are untouched.

**Not about config axes.** `fvp-aemv8r-smp` folds SMP into the name, and the
NuttX crate carries `arm`, `arm-smp` and `riscv` defconfigs where the first two
are one machine in UP and SMP form. SMP is an axis, not a machine. R1 says
nothing about it and a rename must not quietly decide it — see phase-437's
non-goals.

## 4. What the rule selects

| current (index · cmake) | reach | name |
| --- | --- | --- |
| `qemu-arm-baremetal` · `mps2-an385` | part | `mps2-an385-baremetal` |
| `qemu-arm-freertos` · `mps2-an385-freertos` | part | `mps2-an385-freertos` |
| — · `mps3-an536-freertos` | part | `mps3-an536-freertos` (unchanged; add to index) |
| — · `s32z270-freertos` | part | `s32z270-freertos` (unchanged; add to index) |
| `qemu-arm-nuttx` · `nuttx-qemu-arm` | machine | `qemu-armv7a-nuttx` |
| `qemu-riscv-nuttx` · `nuttx-qemu-riscv` | machine | `rv-virt-nuttx` |
| `qemu-riscv64-threadx` · `riscv64-qemu` | machine | `rv-virt-threadx` |
| `qemu-esp32-baremetal` · `esp32-c3` | part | `esp32-c3-baremetal` |
| `threadx-linux` | system | unchanged |
| — · `freertos-posix` | system | unchanged; add to index |
| `zephyr` | meta — Zephyr supplies its own boards | unchanged |
| `native` · `posix` | host | unchanged (CLAUDE.md owns these) |

Sixteen index keys become twelve. The two `rv-virt-*` entries are the same QEMU
machine at different ISA widths and are distinguished by the `arch` field
(`riscv32` vs `riscv64`) — R5 doing its job rather than a collision.

And in vocabulary 3, per R6:

| current fixture `platform =` | rows | name |
| --- | ---: | --- |
| `qemu-arm-baremetal` | 21 | `baremetal` |
| `qemu-esp32-baremetal` | 3 | `esp32` (collapses into the existing token) |

Twelve fixture platforms become eleven.

Every retired name, its replacement and the rule that selected it are
enumerated in
[`docs/reference/retired-board-names.md`](../reference/retired-board-names.md),
which also records the two `mps2-an385` / `esp32-c3` strings that survive
outside the index and the crate directories that deliberately did not move.

## 5. How it is enforced

`check-board-vocabulary` asserts today that every `board=` resolves and is an
index key, and that the marked mirror pairs stay identical. Under this RFC the
mirrors go away, and what replaces the third assertion is a rule about NAMES:

* a board whose cmake overlay or board crate pins a linker script, a memory
  base or a peripheral address is a **part or machine** board, and its name must
  not be an arch or an ISA alone;
* a board that pins none of those is a **system** board, and its name must not
  contain a machine or part;
* no board name contains `qemu` unless the machine's own upstream name does
  (`qemu-armv7a` is NuttX's spelling, not ours).

That gate is phase-437's work item, and it is what keeps the rule from decaying
back into two vocabularies — which is the failure mode this repo has paid for in
`native`/`posix`/`linux`, `[system.*]` vs `[prereq.*]`, and the module-vs-
dispatcher setup spelling.

## 6. Cost of adopting

50 `package.xml` files carry a `board=` export; example directories, board
crates, `cmake/board/*.cmake`, `NANO_ROS_BOARD` fixture coordinates and the
book all spell one. Six `examples/<board>/` directories move. Nothing keys on
the string, so the change is mechanical — which is exactly why it needs a phase
with a staged plan rather than one sweeping commit.
