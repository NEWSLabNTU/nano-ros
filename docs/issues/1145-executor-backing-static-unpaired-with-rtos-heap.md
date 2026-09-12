---
id: 1145
title: "The executor backing is a `.bss` static now, but on an RTOS the allocator
  arena it used to come out of was never lowered — so those images reserve the
  same bytes twice, and nobody has measured it"
status: open
type: tech-debt
area: core
related: [phase-392, RFC-0002, 0163, 0880, phase-448]
---

## Problem

phase-392 W6 moved the executor's per-entry storage off the heap and into a
named `.bss` static (`nros_node::executor::backing::EXECUTOR_BACKING`), because
`mem-report` reads symbols and a `Box::leak` has none.

On a **hosted** target that is free: the allocator is the OS heap, which has no
fixed reservation, so nothing was double-counted and the bytes simply became
visible. Measured on the native zenoh talker — see phase-392 W6.

On an **RTOS** it is not free, because the allocator arena is *itself* a fixed
static sized to hold this backing:

| platform | the knob that already reserves these bytes |
| --- | --- |
| Zephyr | `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` (CLAUDE.md records "executor backing alone needs ~75 KB"; the default is 16 KB, so every Zephyr Rust image raises it) |
| FreeRTOS | `configTOTAL_HEAP_SIZE` / heap_4, which also draws the task stacks |
| NuttX / ThreadX / ESP-IDF | the port's own heap sizing |

Nothing lowers those knobs, so an RTOS image built after W6 carries the
reservation **twice**: once as `.bss`, once as heap headroom it no longer needs.

## Why it was not fixed in W6

The pairing is per-image and the second half can only be established by
building the image and measuring — lowering a heap knob by an argued number is
exactly the "never claim a saving you did not measure" rule this phase carries.
W6 built and measured **one** image, a native host ELF; it built no Zephyr,
FreeRTOS, NuttX, ThreadX or ESP32 image, and says so.

## What W6 left in place instead

An opt-out, so no RTOS image is forced to pay twice while this is open:

```
NROS_EXECUTOR_BACKING_U64S=0
```

emits no static at all and restores the `Box::leak`. A non-zero value overrides
the reservation's size. Read by `nros-node/build.rs` through the same
env → `$DOTCONFIG` reader every other executor knob uses (issue 0460).

**The Zephyr `CONFIG_` half is DECLARED now** — issue 1171 did it, because the
hand-copied subtrahend this issue left behind could not be fixed without it.
The concern recorded here was real and the resolution is not the `bool` this
paragraph guessed at: `int … default -1`, the tree's DERIVE sentinel (issue
0940), which `dotconfig_usize` already reads as "no value" and so leaves the
crate's own sizing in place. `0` keeps its documented meaning of "no static".
An image that lowers its arena STATES the word count, and the reservation is
`8 x` it on every target — see issue 1171 for why a measured number could not
be right for both boards this conf builds for.

## What closing this looks like

For each RTOS platform, in this order, and one platform per commit:

1. Build the platform's talker fixture at HEAD, record `just mem-report <elf> --json`.
2. Lower that platform's allocator-arena knob by the measured
   `EXECUTOR_BACKING` size, rebuild, and confirm the image still **boots and
   delivers** — not merely links. An under-sized allocator fails at the first
   allocation the executor no longer makes but something else still does.
3. Record the before/after in phase-392 W6's table with the exact command.

Do not do this as one sweeping commit: the failure mode is a runtime allocation
failure on one platform, and a six-platform diff makes that unattributable.

## Zephyr, one leaf: DONE 2026-09-06

`examples/zephyr/rust/talker` (zenoh), both boards it builds for. `malloc_arena`
is an `nm`-visible symbol, so both halves of the pairing read off one ELF.

| symbol | before | after |
| --- | ---: | ---: |
| `malloc_arena` (mps2_an385) | 1,048,576 | 961,320 |
| `EXECUTOR_BACKING` (mps2_an385) | 87,256 | 87,256 |
| `EXECUTOR_BACKING` (native_sim/native/64) | 88,328 | 88,328 |

Lowered by the SMALLER of the two boards' backings (87,256), so neither board
loses headroom it had before W6.

Whole-image RAM on mps2/an385, west's own report:

```
before   RAM: 1789196 B / 4 MB  (42.66%)
after    RAM: 1701940 B / 4 MB  (40.58%)
delta         -87,256 B
```

Exactly the backing size. **Boots and delivers, not merely links**:
native_sim published 20 messages in 25 s — identical to the pre-change control
run — and a stock ROS 2 consumer received them
(`ros2 topic echo /chatter std_msgs/msg/String --once` -> `data: 'Hello World: 9'`).

### Found on the way, and fixed first

The Zephyr build could not run at all. Two blockers, both filed and neither
this issue's:

* **issue 1167** — a red on `main` from the same day: `#if ZPICO_MAX_SESSIONS < 1`
  sat ABOVE the knob's default, so every Zephyr zenoh image failed to compile.
  Two sessions found it hours apart from opposite ends and fixed it in opposite
  directions; `91f3ce7c9` is the one that shipped, and the gate that asks
  whether a guard can FIRE is on its own branch.
* **issue 1166** (filed the same day, on its own branch) — the self-hosted
  runner builds Zephyr into a developer checkout and claims the shared west
  build dir, so `west` refused this measurement's first build. Worked around
  here with a private `-d`; that issue is the fix.

## Zephyr, swept: DONE 2026-09-06

All twelve Zephyr Rust confs whose image actually has a picolibc arena now pair
it, using the STATED mechanism from issue 1171 rather than a copied `nm` figure:

```
# nros-arena-base: <base>
CONFIG_NROS_EXECUTOR_BACKING_U64S=11041
CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=<base - 88328>
```

`8 * 11041 = 88,328` is the derived size on the larger of the two targets a
Zephyr Rust leaf builds for, so it is at or above the requirement on both, and
`nros-node` refuses to compile naming the knob if it ever is not.

No leaf overrides an executor knob (`MAX_CBS`, `MAX_SC`, `MAX_NODES`, the
arena), which is why one number serves all twelve.

| base | new arena | confs |
| ---: | ---: | --- |
| 1,048,576 | 960,248 | 6 zenoh |
| 16,777,216 | 16,688,888 | 6 cyclonedds |

Verified by BUILDING, since the compile-time refusal is the check:

| image | `EXECUTOR_BACKING` | `malloc_arena` |
| --- | ---: | ---: |
| `action-server` / zenoh / mps2_an385 | 88,328 | 960,248 |
| `talker` / cyclonedds / native_sim | 88,328 | 16,688,888 |

`action-server` is the heaviest role in the tree, so a stated size that suffices
there suffices for every lighter role at the same base. Whole-image RAM on
mps2/an385: **1,701,948 B / 4 MB (40.58 %)** — level with the already-paired
talker's 1,701,940, which is the point.

### The six XRCE confs were REVERTED, and that is the finding

They set `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` and their images **do not select
picolibc**, so the symbol never reaches the resolved `.config`. Pairing them
would have been arithmetic over a knob that does not exist — and it PASSED the
textual gate, which cannot know. Caught by building one and reading its
`.config`. Filed as issue 1189.

`EXECUTOR_BACKING` is present in the XRCE image (88,328 B, same as its
siblings), so W6 applies there; what is absent is the fixed arena those bytes
used to come out of. There is nothing to lower, and "state nothing" is correct.

## NuttX: NOTHING TO PAIR, and it is measured — 2026-09-12 (phase-448 W5)

NuttX's heap is **not a reservation**. In the flat build `up_allocate_heap`
returns

```c
  uintptr_t base = g_idle_topstack;          /* end of .bss + the idle stack */
  size_t    end  = CONFIG_RAM_END;
  *heap_start = (void *)base;
  *heap_size  = end - base;
```

(`arch/arm/src/common/arm_allocateheap.c:112`, and the RISC-V twin
`arch/risc-v/src/common/riscv_allocateheap.c:69`, which is the same two lines).
So the heap is a boot-time LEFTOVER computed from where `.bss` ended, not a
number anybody sized: a byte added to `.bss` is a byte removed from the heap,
automatically and exactly. The backing is paid for ONCE, and there is no knob to
lower — the `CONFIG_RAM_SIZE` in the defconfigs describes the part's RAM, not a
budget for anything.

MEASURED rather than argued, on `examples/qemu-armv7a-nuttx/rust/talker` built
twice through `scripts/build/fixtures-build.sh nuttx rust`:

| | `EXECUTOR_BACKING` | `_ebss` | `g_idle_topstack` | heap |
| --- | ---: | --- | --- | ---: |
| default (static on) | 87,496 | `0x4027a000` | `0x4027b000` | 131,616,768 |
| `NROS_EXECUTOR_BACKING_U64S=0` | 0 | `0x40265000` | `0x40266000` | 131,702,784 |

The heap moved 86,016 B against a backing of 87,496 B; the 1,480 B difference is
the 4 KiB alignment of `_ebss`, not a discrepancy — `0x4027a000 - 0x40265000` is
exactly `0x15000`.

RUNNING IMAGE (the bar this issue sets, not merely linking):
`test_rtos_pubsub_e2e / Nuttx / Rust` PASSES on the default build, on QEMU
against a live router, 120 s.

No code changed for this port. The finding is recorded in
`check-executor-backing-arena-pairing`'s `PORTS` ledger as `none` WITH this
measurement, so that "needs nothing" and "nobody has looked" stop reading the
same — which is how this issue's own "Still open" list went stale.

## ESP32: ALREADY PAIRED, and the pairing is not this issue's arithmetic — 2026-09-12 (phase-448 W5)

This port was paired on 2026-09-10 by `ffc614252 fix(esp32): the heap was sized
to hold the executor arena, and the arena moved to .bss`, which found it from the
other end — the nightly `check-stack-floor` red — and did the work this issue
asks for, including running the whole esp32 QEMU suite. The "Still open" list
below said ESP32 was untouched for two days after that.

`nros-board-esp32-qemu/src/node.rs`'s `esp_alloc::heap_allocator!(size: 48 *
1024)` became `16 * 1024`, a reduction of **32,768 B**, against a measured
29,400 B backing on `esp32_entry`.

RE-MEASURED here on the two single-node images, which are what this host can
build (`nm -S`, `riscv32imc-unknown-none-elf`, `nros-relwithdebinfo`):

| image | `EXECUTOR_BACKING` | `init_hardware::HEAP` | `.stack` |
| --- | ---: | ---: | ---: |
| `esp32_qemu_talker` | 24,696 | 16,384 | 102,984 |
| `esp32_qemu_listener` | 24,696 | 16,384 | 72,720 |

(The third `.bss` heap in these images, `nros_platform_esp32_qemu::memory::HEAP`
at 34,480 B, is zenoh-pico's `z_malloc` arena, not the Rust global allocator.
It never held the executor backing and is not part of this pairing.)

**It is deliberately NOT expressed as `base - 8 * words`**, and that is a fact
about the fix rather than a gap:

* the 32,768 B was chosen as a RETURN to the pre-phase-271 heap — the value whose
  only recorded failure was the 17,032 B allocation that is now the static — not
  as the backing's size. It is 3,368–8,072 B LARGER than the backing, depending
  on the image;
* a gate asserting the identity would have to be told to expect an inequality,
  and an inequality here cannot tell a deliberate margin from a stale number;
* what makes the margin safe is the FAILURE MODE. The executor takes the heap arm
  only when its backing does not fit the reservation, and that arm dies loudly at
  `Executor::open` ("memory allocation of N bytes failed"), never as a silent
  overflow. And on esp32-c3 `.stack` is the linker leftover after `.bss`, so
  `check-stack-floor` — which runs per row on every esp32 fixture build — catches
  the over-reservation direction on every build.

So the ledger records `esp32: stated-heap` with that reasoning, rather than
adding an arithmetic the fix does not have.

RUNNING IMAGES, re-verified on this host:

```
PASS [ 6.421s] nros-tests::esp32_emulator test_esp32_qemu_talker_boots
PASS [11.879s] nros-tests::esp32_emulator test_esp32_talker_listener_e2e
```

### Two reds stood between `main` and a built esp32 image

Neither is this issue's, and both are why re-measuring was not a five-minute job:

* **issue 1344** — `shim/qos.rs`'s report-once latches used
  `core::sync::atomic::AtomicBool`, which has no `swap` on riscv32imc. No esp32
  image compiled.
* **issue 1346** — `check-stack-floor.py`'s own coverage assertion was red
  (`threadx-riscv64` missing from `ROW_PLATFORM_BOARD`), and it runs per fixture
  ROW, so it failed rows that had already linked.

Both fixed; both reached `main` because no merge-gating lane builds an esp32
image.

The `esp32_entry` workspace cell was NOT rebuilt here: `nros build
demo_bringup:esp32` wants `rustup target add riscv32imc-unknown-none-elf` on a
toolchain this host does not have it on. Its numbers above are ffc614252's.

## ThreadX: PAIRED on threadx-linux — 2026-09-12 (phase-448 W5)

ThreadX has no global heap. `nros_platform_alloc` forwards to `tx_byte_allocate`
on ONE pool, whose storage is `byte_pool_storage[4 * 1024 * 1024]` — a fixed
`.bss` array in `nros-board-common/c/threadx_hooks.c` — and `nros-platform`
installs the tree's single `#[global_allocator]` over it. Before phase-392 W6
the executor's per-entry storage was a `Box::leak` out of exactly there. So this
port double-reserves, the way Zephyr did.

MEASURED at HEAD, `scripts/build/fixtures-build.sh threadx-linux rust`, `nm -S`:

| leaf | `EXECUTOR_BACKING` | `byte_pool_storage` |
| --- | ---: | ---: |
| talker / listener / service-server | 23,336 | 4,194,304 |
| action-client / action-server | 29,008 | 4,194,304 |
| service-client | **35,952** | 4,194,304 |

The sizes differ per role because `nros sync` derives each leaf's executor knobs
(the issue 0827 / 1061 channel), exactly as issue 1197 measured on FreeRTOS.

### The mechanism, and why it is the ladder rather than a second knob

The statement is `[board.knobs.executor] backing_u64s` in the board's
`nros-board.toml` — the RFC-0049 rung, with the existing
`NROS_EXECUTOR_BACKING_U64S` env front-end still outranking it. It is read
TWICE and written ONCE:

* `nros-node/build.rs` sizes `EXECUTOR_BACKING` from it (`env_opt_usize_laddered`);
* `threadx_sources::add_threadx_hooks_source` resolves the same rung and passes
  `-DNROS_EXECUTOR_BACKING_U64S` to the C compile, where
  `BYTE_POOL_SIZE = BYTE_POOL_BASE_SIZE - 8 * NROS_EXECUTOR_BACKING_U64S`.

So the SUBTRAHEND is never written down a second time. That is the difference
from what issue 1171 had to fix on Zephyr, where the arena's new value is a
second number in the conf and a gate has to check the sum.

Issue 1197 established that a board crate cannot DERIVE the backing size — it is
`arena + repr(Rust) tables`, and probing `nros-node` from outside its dependency
graph gets the wrong features and env (87,256 against a linked 21,832). Nothing
here derives anything; it forwards a stated board fact. `nros-node`'s const
assertion is what makes the statement safe: a value below the executor's own
sizing is a compile error naming the knob.

### The number, and what it costs

`backing_u64s = 4494` = `35,952 / 8`, the heaviest role on this board. Below it
and the build fails loudly; above it and the bytes are wasted. Every leaf's
static becomes that size and its pool shrinks by the same amount, so:

| | before | after |
| --- | ---: | ---: |
| `talker` backing + pool | 23,336 + 4,194,304 = 4,217,640 | 35,952 + 4,158,352 = **4,194,304** |
| `service-client` backing + pool | 35,952 + 4,194,304 = 4,230,256 | **4,194,304** |

**Verified on all six leaves: `backing + pool == 4,194,304` exactly** — the
pre-W6 reservation, paid once. The per-image saving is exactly that image's old
backing.

The cost of one number per board rather than one per leaf is that four of the
six roles carry a static larger than they need. They give back the same bytes
from the pool, so no image grows; what it does mean is that `mem-report` shows
the board's worst case rather than the leaf's. Zephyr made the same trade for
the same reason (one stated number, twelve confs).

RUNNING IMAGES on the paired build, `rtos_e2e` / ThreadxLinux / Rust:

```
PASS [ 48.203s] test_rtos_service_e2e
PASS [119.553s] test_rtos_pubsub_e2e
FAIL           test_rtos_action_e2e   <- issue 1343, pre-existing
```

The action cell is issue 1343 and is not this: the control — the same lane
rebuilt with `NROS_EXECUTOR_BACKING_U64S=0`, no static at all, pool at its base
— fails identically, at QoS validation, before any allocation.

### Not done: threadx-riscv64

The mechanism is board-agnostic and the rv64 descriptor can state its own rung
the moment someone measures it. It is NOT stated here because the number has to
come from that board's own images (`nm -S` on a riscv64 build), and building the
rv64 lane was not affordable in this session. Unstated is the safe state: no
define, the C `#ifndef` default of 0 applies, and the pool stays at its base —
the image pays twice, exactly as it does today.

A C or C++ ThreadX image is in that state permanently and correctly: its
`nros_executor_t` objects are file-scope statics and never came out of this
pool, so it has nothing to give back.

### Found on the way, and not fixed

`BYTE_POOL_BASE_SIZE` is 4 MiB because `threadx_hooks.c` has said "4 MB byte
pool" since phase 152. Nobody derived it. The pairing subtracts a measured
number from an undeclared one, which is correct arithmetic on a base that is
itself a guess — worth its own work item, alongside the FreeRTOS heap budget.

## Still open

* ~~Every other Zephyr Rust leaf.~~ **DONE** — see the sweep above. Twelve confs
  paired; the six XRCE ones deliberately not, because their images have no
  picolibc arena to pair (issue 1189).
* ~~**NuttX**~~ **DONE 2026-09-12** — nothing to pair, measured. See above.
* **FreeRTOS** — untouched; phase-448 W3/W4, and issue 1197 has the measurement
  and the layering blocker.
* ~~**ESP32**~~ **DONE** — paired by `ffc614252` on 2026-09-10 and re-measured
  and re-run 2026-09-12. See above.
* **ThreadX** — `threadx-linux` PAIRED 2026-09-12 (see below); `threadx-riscv64`
  still unstated, and unstated is the safe state. The mechanism is in place for
  it; what is missing is a measurement on that board's own images.
* **The byte pool's 4 MiB base, and FreeRTOS's 2 MiB, are undeclared numbers.**
  Both pairings subtract a measured size from a base nobody derived.
* ~~**The subtrahend is a hand-copied literal.**~~ RESOLVED —
  [issue 1171](archived/1171-arena-backing-pairing-is-hand-maintained.md).

## Related

- phase-392 W6 — the change that created this, and the measurement that is done.
- Issue 0163 (archived) — where the "~75 KB from the picolibc arena" figure comes from.
- Issue 0880 — the other half of amendment A: a named section for the static
  (`NROS_EXECUTOR_BACKING_SECTION`) exists but nothing places it yet.
