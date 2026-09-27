---
id: 1512
title: "The C/C++ API does not reach either bare-metal family, and the reason
  on record — \"assumes a hosted RTOS for startup, heap, libc, RNG, clock\" —
  is no longer true: what is missing is a `nros-c`/`nros-cpp` platform arm and
  a board that declares the heap it already has"
status: open
type: tech-debt
area: [build, cmake, memory]
related: [rfc-0026, rfc-0034, rfc-0042, rfc-0064, rfc-0093, 0038, 0594, 0617]
found: 2026-09-27
---

## What happens

Both bare-metal example families are Rust-only, and nothing in the tree asks
why:

```text
examples/mps2-an385-baremetal/rust/   13 leaves   (+ README.md, no c/, no cpp/)
examples/esp32-c3-baremetal/rust/      2 leaves   (+ README.md, no c/, no cpp/)
```

Tracked `.c`/`.cc`/`.cpp`/`.h`/`.hpp` files under either family: **zero**. The
sibling `examples/mps2-an385-freertos/` — the *same silicon*, same QEMU
machine, same Cortex-M3 toolchain — has `c/` (6 leaves) and `cpp/` (6 leaves)
beside its `rust/`, each with an ordinary `CMakeLists.txt` calling
`nano_ros_add_executable`. The difference between the two trees is the RTOS,
not the board.

So "nano-ros has a C API" is true on five platforms and silently false on the
one class of target the project's positioning leans on hardest. And it is
invisible: no gate asks, because there is no example to build.

## The reason on record is stale

`examples/README.md`'s known-gaps table already carries the row, with a cause:

> `mps2-an385-baremetal/{c,cpp}/*` — No bare-metal C/C++ example harness
> exists. `nros-c` / `nros-cpp` ship as static libs but assume a hosted RTOS
> for **startup, heap, libc, RNG, and clock** — none of which are wired on
> `mps2-an385-baremetal`'s pure Cortex-M3 runtime.

Every one of those five is wired today. Measured on `origin/main`:

| Claimed missing | Where it lives now |
| --- | --- |
| heap | `nros-platform-mps2-an385/src/memory.rs` — a `zpico_alloc::FreeListHeap<HEAP_SIZE>` static in `.bss`, 128 KB default, `NROS_HEAP_SIZE` compile-time override, reached by `impl PlatformAlloc for Mps2An385Platform` |
| libc | `nros-baremetal-common/src/libc_stubs.rs`, on by default (`nros-platform-mps2-an385` `default = ["libc-stubs", "libc-heap"]`) |
| RNG | `nros-baremetal-common/src/random.rs` |
| clock | `nros-platform-mps2-an385` `cffi-export` expands `nros_platform_export!`, which emits `nros_platform_clock_ns` |
| startup / link | `cmake/board/nano-ros-board-mps2-an385-baremetal.cmake` — `nros_board_link_app()` sets `-T…/mps2-an385.x -Wl,--gc-sections -nostartfiles` |

The last row is the one worth quoting, because that file says out loud what it
is for and that nobody has used it:

> it has ZERO live consumers: `NANO_ROS_BOARD` is set to this value nowhere in
> the tree. It survives as **the C/C++ seam** described below […] this overlay
> surfaces the same file under `nros_board_link_app(target)` **for C/C++
> consumers that bypass the Rust crate path**.

The platform module above it, `cmake/platform/nano-ros-baremetal.cmake`
(phase-138), is complete: `NanoRos::Platform`, `nros_platform_baremetal_iface`,
`NROS_PLATFORM_LINK_FEATURES`, and a `nros_platform_link_app()` that dispatches
to the board overlay with a `FATAL_ERROR` naming `cmake/board/` when it cannot.
The C road to bare metal was built and then never driven.

RFC-0034 D6 settled the allocator question outright, and its own table names
this platform:

| Platform | Framework allocator? | nano-ros `global-alloc` | Rust heap owner |
|---|---|---|---|
| bare-metal (MPS2 / STM32F4 / ESP32-bare) | no | **on** → wraps `nros_platform_alloc` | nano-ros |

`nros-board-mps2-an385/Cargo.toml` implements exactly that today —
`nros-platform = { …, features = ["platform-mps2-an385", "global-allocator",
"critical-section"] }` — and the C-side half exists too: `nros/platform.h`
gates a canonical `malloc`/`free` shim over the CFFI alloc/dealloc on
`NROS_PLATFORM_HAS_MALLOC`, which `nros_board_capability_defines()` lowers from
`[board.capabilities] heap = true`.

So **this is not a port.** It is wiring, plus one declared fact that is wrong.

## What is actually missing

### 1. `nros-c` and `nros-cpp` have no bare-metal platform arm

Both crates carry exactly five: `platform-posix`, `-zephyr`, `-freertos`,
`-nuttx`, `-threadx`. Measured:

```text
$ cargo check -p nros-c --no-default-features --features platform-bare-metal
error: the package 'nros-c' does not contain this feature: platform-bare-metal
help: packages with the missing feature: nros-rmw-zenoh, nros-rmw-zenoh-staticlib
```

The help line is the whole shape of the gap. `platform-bare-metal` is a real
feature in this tree — `nros-rmw-zenoh` has it, `nros-rmw-zenoh-staticlib`
forwards it, two board crates and two test bins select it, and
`PlatformKind::{BareMetal,Esp32,Stm32,OrinSpe}::platform_feature()` returns it.
It stops at the two language-surface crates.

`platform-freertos` is the template the other four arms follow, and it is four
lines:

```toml
platform-freertos = [
    "nros-log/platform-clock",
    "global-allocator",
    "nros-platform/platform-freertos",
    "nros-rmw-zenoh?/platform-freertos",
]
```

Three of those four transcribe unchanged — `nros-log/platform-clock` (the
`cffi-export` emits the symbol), `global-allocator` (RFC-0034 D6 says on), and
`nros-rmw-zenoh?/platform-bare-metal` (already exists). **The third line is the
decision**, because bare metal is the one platform where `nros-platform` has no
single name for itself: it has three board-specific ones,
`platform-mps2-an385`, `platform-stm32f4`, `platform-esp32-qemu`. So the arm is
either

* **one arm per bare-metal board** on `nros-c`/`nros-cpp`, mirroring
  `nros-platform`'s own shape and making the C staticlib root name a board —
  which is the thing RFC-0064 argues a platform layer should not do; or
* **one `platform-bare-metal` arm that omits the `nros-platform/platform-X`
  line entirely**, leaving the concrete platform to whoever brings the board
  crate — which is what the Rust road already does (`nros-board-mps2-an385`
  selects `platform-mps2-an385` unconditionally) and what
  `nros-cli-core/src/builder/entry.rs` already special-cases the bare-metal
  family for, in a comment that describes this exact split.

Present both; this issue picks neither. Note what the second costs: `nros-c` is
the staticlib link root, so an arm that selects no platform is only correct if
something else in the image's cargo graph does — and for a C leaf that means
the generated entry crate must name the board, which is a CMake-road question
this issue does not answer.

### 2. Three spellings of one platform, and a build that falls through

`nros_feature_set()`'s PLATFORM ladder has no bare-metal arm. A bare-metal call
lands in the catch-all:

```cmake
elseif(_cross)
    # Unknown embedded cross target: no_std + alloc, matching the board tier
    list(APPEND _feats alloc "platform-${_FS_PLATFORM}")
```

The good news is that this fails hard rather than building something wrong:
whatever `${_FS_PLATFORM}` interpolates to is a feature `nros-c` does not have,
and cargo refuses. The bad news is *what* it interpolates to, because the tree
spells this platform three ways and the three do not meet:

| Spelling | Where | Reached as |
| --- | --- | --- |
| `bare-metal` | `config/bare-metal/nros-platform.toml` `names`, `nros-board.toml` `platform =`, `PlatformKind::kebab()`, the `nros-rmw-zenoh` feature | the cargo-feature / descriptor namespace |
| `baremetal` | `cmake/platform/nano-ros-baremetal.cmake`, `PlatformKind::BareMetal::cmake_deploy()` | `NANO_ROS_PLATFORM`, hence `_FS_PLATFORM` |
| `esp32` | `config/bare-metal/nros-platform.toml` `names[1]` (phase-468 W1), `PlatformKind::Esp32` | a second name for the same descriptor |

So the catch-all would ask for `platform-baremetal` while every other road in
the tree says `platform-bare-metal`. A new arm has to pick one and make the
ladder say it, or the two roads will disagree the first time a C leaf
configures.

And for the second family it is worse than a spelling: `cmake_deploy()` returns
`None` for `PlatformKind::Esp32`, documented as "`None` for a platform with no
C/C++ platform module". The CLI has already *declared* that esp32 has no C/C++
road, so `examples/esp32-c3-baremetal/c/` is blocked by an authored decision
rather than by missing code. `examples/README.md` agrees and gives a different
reason again — that C/C++ on that chip belongs under a hypothetical
`esp32-idf/` tree, not here. Those two want reconciling before anyone writes a
leaf.

### 3. The board declares it has no heap, and it has one

This is the sharpest of the three and the only one that is a plain
contradiction:

```toml
# packages/boards/nros-board-mps2-an385/nros-board.toml
# RFC-0042 D2 / phase-241 wave C — declared capabilities (SSoT). Pure bare-metal:
# no heap (static only). A heap-capable bare-metal board sets heap = true.
[board.capabilities]
heap = false
```

The same board runs a 128 KB `FreeListHeap` in `.bss`, exports it through
`PlatformAlloc`, installs it as the Rust `#[global_allocator]` via its own
`nros-platform` feature list, and defines `malloc`/`free`/`realloc`/`calloc` as
C symbols by default (`libc-heap`). RFC-0034 D6's table row for this platform
reads `on`.

The consequence is precisely a C/C++ one, which is why nobody has hit it: with
`heap = false`, `nros_board_capability_defines()` emits no
`NROS_PLATFORM_HAS_MALLOC`, and `nros/platform.h` then makes a TU that uses the
`nros-cpp` heap containers fail to **compile** — the issue-0038 guard, working
exactly as designed, on a board that has a heap. The Rust leaves never consult
this file, so the wrong value has cost nothing for as long as the family has
been Rust-only.

The other bare-metal board disagrees with it, too: `nros-board-esp32-qemu`
declares `heap = true`, `atomics = true`, `threads = true`.

Fixing this row is a prerequisite for (1), not a follow-up: the arm can be
perfect and a C leaf still will not compile.

## What each option costs

The allocator decision this issue was opened to pose is already made
(RFC-0034 D6), so the remaining choice is narrower and worth stating plainly,
because it is a capability statement and not a build flag:

* **Declare the heap (`heap = true`) and ship the full C surface.** The 128 KB
  arena is RAM the image already declares; nothing new is spent, and the
  declaration merely stops lying. `param-services` and `lifecycle-services`
  become offerable — both `compile_error!` in `nros-c` without `alloc`
  (`"`param-services` allocates: add \"alloc\" to this crate's features"`).
* **Keep `heap = false` and offer an alloc-free C profile**, in which the
  allocating capabilities are compile-time absent on this board. That is a
  coherent position — `check-no-alloc-image --tier heap-free` exists and
  `libc-heap` was split out precisely so a heap-free image can drop the
  symbols — but it means nano-ros's C API on bare metal is a strict subset of
  its C API elsewhere, permanently, and that belongs in the book rather than in
  a board file.

They are not exclusive per board: mps2 could take the second and esp32-qemu
(already `heap = true`) the first. What is not tenable is the present state,
where the board declares one and implements the other.

## Why it matters

The C/C++ surface is what a bare-metal integrator reaches for, and the claim
that it exists is load-bearing in the positioning. It is unfalsifiable today:
there is no leaf, so no `fixtures.toml` row, so no lane, so no gate — the
"Rust-only leaf" class a layout study identified is invisible to every check in
the repo. Meanwhile the documented reason for the gap has been overtaken by
five separate pieces of work (RFC-0034 D6, phase-138's CMake seam, phase-241
wave C's capability lowering, `nros-baremetal-common`, phase-391 W5's
`libc-heap` split) and now points a reader at a port that does not need doing.

## Acceptance

* A C leaf under `examples/mps2-an385-baremetal/c/` that builds and runs in
  QEMU the way the `rust/` leaves do, with a `fixtures.toml` row and a lane
  that runs it. Ideally a `cpp/` sibling too.
* `nros-c` and `nros-cpp` carry a bare-metal platform arm, and
  `nros_feature_set()`'s PLATFORM ladder names it rather than reaching it
  through the `_cross` catch-all.
* One spelling. `bare-metal` vs `baremetal` is resolved, or the two namespaces
  are documented as deliberately distinct at the `cmake_deploy()` boundary that
  already translates between them.
* `nros-board-mps2-an385`'s `[board.capabilities] heap` agrees with what the
  board's platform crate does, whichever direction that is settled in.
* `examples/README.md`'s known-gaps row is rewritten or deleted — as it stands
  it is a wrong diagnosis aimed at the next person.
* The esp32 question is answered explicitly: either `cmake_deploy()` gains a
  token for it, or the "no C/C++ platform module" decision is written down
  somewhere a reader of `examples/esp32-c3-baremetal/` will find it.

At that point the "Rust-only leaf" class dissolves into the ordinary
standalone-leaf class of RFC-0026, and the C API's platform list stops needing
an asterisk.
