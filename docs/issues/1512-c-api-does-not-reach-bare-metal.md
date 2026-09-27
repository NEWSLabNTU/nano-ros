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

---

## What landed (phase-470 W6, 2026-09-27) — and where this issue's analysis was wrong

**A C leaf on `mps2-an385-baremetal` now builds and boots.** Measured:

```text
$ nros sync examples/mps2-an385-baremetal/c/talker
$ (cd examples/mps2-an385-baremetal/c/talker && cargo build --release)
    Finished `release` profile [optimized] target(s)
$ arm-none-eabi-size …/baremetal-c-talker
   text    data     bss     dec     hex
 380576    4980  544080  929636   e2f64
$ qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -icount shift=auto \
    -semihosting-config enable=on,target=native -kernel …/baremetal-c-talker
  nros QEMU Platform
Initializing LAN9118 Ethernet…   MAC: 02:00:00:00:00:00
Creating network interface…      IP: 192.0.3.10
Ethernet ready.
[ERROR] nros: [2.702035] zpico Session -> ConnectionFailed
nros: application complete
```

The board comes up, the C `nros_support_init` runs, the zenoh backend registers,
and the session open fails because nothing is listening at the backend's default
locator — which is the one thing still missing, below.

### Correction 1 — for the C ROAD specifically, this WAS partly a port

The headline said "this is not a port. It is wiring, plus one declared fact that
is wrong." That is exactly right for the RUST half, and it is now proven: with
the new arm, `nros-c` AND `nros-cpp` build clean staticlibs for
`thumbv7m-none-eabi` with no source change anywhere.

It is **not** right for a C-ROOTED image, and the reason is structural rather
than a missing file. Every other platform's C road works because the RTOS
supplies three things in C — a startup, a `main`, and a C platform port
(`packages/platform/nros-platform-freertos/src/{platform,net,timer}.c` plus the
board's `board_mps2.c` / `network_glue.c`). Bare metal has **none of them in C**:
`packages/platform/nros-platform-mps2-an385/` contains zero `.c` files, its
`nros_platform_*` symbols come from `nros_platform_export!` in Rust, and the
LAN9118 + smoltcp bring-up that must run before any of them work is
`nros_board_mps2_an385::init_hardware`, reachable only from Rust.

So the CMake road is further from usable than "built and never driven" suggests,
and the board overlay's own linker line is the clearest evidence:
`nros_board_link_app()` passes
`-T…/packages/boards/nros-board-mps2-an385/mps2-an385.x`, and that file is a
`MEMORY { … }` fragment with symbol assignments and **no `SECTIONS`, no `ENTRY`**
— it is the `memory.x` the board's `build.rs` copies into `OUT_DIR` for
`cortex-m-rt`'s `link.x` to `INCLUDE`. Passing it as the whole script, with
`-nostartfiles`, produces an image with no vector table. A C-rooted bare-metal
road needs a `SECTIONS` script, a reset/vector startup, and a C-callable board
init — three pieces, none of which exist, and the third cannot live in `nros-c`
without the staticlib root naming a board (the inversion RFC-0064 forbids).

The leaf therefore takes the only shape such a board admits: the LINK ROOT is
Rust and the C application is compiled into it. `src/main.rs` owns
`#[cortex_m_rt::entry]` and `run_bare`, `build.rs` compiles `src/talker.c` with
`cc`, and `NROS_APP_MAIN_REGISTER()` emits the `void app_main(void)` that
`<nros/app_main.h>` has always documented for this platform ("per-platform
startup chains call this after platform init").

### Correction 2 — the `heap = false` consequence was the OPPOSITE way round

The issue said `heap = false` makes the issue-0038 guard reject an `nros-cpp`
heap-container TU on a board that has a heap. That is what it *would* do, and it
was not what it did, because a second declared fact was also missing: **nothing
defined `NROS_PLATFORM_BAREMETAL`** for `NANO_ROS_PLATFORM=baremetal`. Neither
`cmake/platform/nano-ros-baremetal.cmake` nor either board overlay set it, so
`<nros/platform.h>` took its hosted default arm and defined
`NROS_PLATFORM_HAS_MALLOC` unconditionally — for the one platform the macro
exists to name. Two wrong facts that cancelled. Fixing the heap row alone is
safe; fixing the platform row alone would have produced the failure the issue
described. Both moved together, in that order.

### What landed

1. **`platform-mps2-an385` / `platform-stm32f4` / `platform-esp32-qemu` arms on
   `nros-c` and `nros-cpp`** — shape **(a)**, per board. Shape (b) (one
   `platform-bare-metal` arm omitting the `nros-platform/platform-X` line) was
   MEASURED and does not compile: `nros-platform` gates `ConcretePlatform` on
   having some `platform-*`, and the Rust road only gets away with omitting it
   because the entry crate, the board crate and `nros-platform` sit in ONE cargo
   resolve. `nros-c` is imported by Corrosion as its own cargo ROOT; the board
   crate is not in it and cannot be, so the selection is spelled there or
   nowhere. The two roads differ because their LINK ROOTS differ, not because
   one of them is wrong.
2. **A bare-metal arm in `nros_feature_set()`'s PLATFORM ladder**, resolving the
   board-specific feature through `cmake/NanoRosBareMetalPlatform.cmake`. It
   accepts BOTH `baremetal` and `bare-metal`. `BOARD` is back in the parse (as an
   optional argument, falling back to the ambient `NANO_ROS_BOARD`): phase-405 W1
   removed it correctly when nothing used it, and bare metal is the condition
   changing rather than a revert of that reasoning.

   No leaf exercises this arm (the leaf below is cargo-rooted), so it was
   measured directly, in `cmake -P` script mode over a throwaway file that
   includes the module and calls the function:

   ```text
   -- baremetal/c:    ros-humble;rmw-cffi;alloc;platform-mps2-an385
   -- baremetal/cpp:  ros-humble;rmw-cffi;alloc;platform-mps2-an385
   -- bare-metal/c:   ros-humble;rmw-cffi;alloc;platform-mps2-an385
   -- bare-metal/cpp: ros-humble;rmw-cffi;alloc;platform-mps2-an385
   -- baremetal/c on esp32-c3-baremetal:
                      ros-humble;rmw-cffi;alloc;platform-esp32-qemu
   ```

   and both refusals fire and name the remedy — an unset `NANO_ROS_BOARD`
   ("Known: mps2-an385-baremetal, esp32-c3-baremetal") and an unknown one ("Add
   the row to cmake/NanoRosBareMetalPlatform.cmake together with the matching
   `platform-*` arm"). Every emitted feature is one both crates declare, which
   `check-baremetal-platform-arms` is what keeps true.
3. **The spelling question is answered as a namespace boundary, not a rename.**
   Measured: 1759 occurrences of `baremetal` and 2242 of `bare-metal`, so
   converging the tree is a ~4000-site rename across every doc series — and
   `baremetal` is load-bearing as the `-baremetal` stack suffix RFC-0093 R2 puts
   in every board NAME (`mps2-an385-baremetal`, `esp32-c3-baremetal`), while
   `bare-metal` is the cargo-feature / descriptor spelling. `cmake_deploy()` is
   where they meet, and it already translates two others of the same kind
   (`Posix` -> `native`, both ThreadX kinds -> `threadx`). Written down at
   `cmake_deploy()`, at the ladder arm, and in the new module.
4. **`nros-board-mps2-an385` declares `heap = true`**, with the reasoning for
   preferring that over a permanently-subset alloc-free profile: the declaration
   is about what the BOARD provides, and dropping the capability is an IMAGE
   decision that is already expressible and already exercised
   (`packages/testing/nros-tests/bins/heap-free-poc-mps2` +
   `check-no-alloc-image --tier heap-free`, which reaches the platform crate with
   `default-features = false` and never consults this file). It was the only
   board of fifteen declaring `heap = false`. The bare-metal platform module now
   also declares `NROS_PLATFORM_BAREMETAL`, and the board overlay lowers its
   capabilities through `nros_board_capability_defines()` — the call
   `nano-ros-board-rv-virt-threadx.cmake` has made for this same class of board
   since phase-241 wave C.
5. **`links = "nros_c"` on `nros-c`**, publishing `DEP_NROS_C_INCLUDE` /
   `_CONFIG_INCLUDE` / `_PLATFORM_INCLUDE`. The paths are the smaller half: the
   `links` key is what makes cargo run a consumer's build script AFTER the one
   that writes `nros_config_generated.h`, instead of racing it — the 0088/0268
   ordering class, which cmake solves with a ninja edge and cargo solves with
   this. (Include ORDER is load-bearing too: the committed
   `nros-c/include/nros/nros_config_generated.h` is a stub that defines nothing,
   so the per-build dir must come first or every generated message header hits
   its own fail-closed `#error` — whose text names the wrong cause.)
6. **`nros-c` / `nros-cpp` in `nros sync`'s crate-path table.** A cargo-rooted C
   image names `nros-c = { version = "*" }` the way every Rust leaf names `nros`;
   without the row sync skipped it as an unknown runtime crate and cargo then
   said `no matching package named 'nros-c' found` — two messages neither of
   which names the table.
7. **Gate `check-baremetal-platform-arms`** (fast line), both directions over
   four rules, with the bare-metal feature SET derived from `nros-platform`'s own
   manifest (a `platform-*` arm naming a `dep:nros-platform-<board>` other than
   the shared `-cffi`/`-api` crates), so a fourth bare-metal board is asked for
   without editing the gate.
8. **`example_portability.rs`'s glue/logic classifier asks the rule, not a
   proxy.** `main.rs` was glue iff a `lib.rs` existed beside it; it is glue iff
   the node logic is in another file, in any language that walk reads. Measured:
   zero existing packages change classification, and the 11 pre-existing
   `rust/{talker,listener,service-client}` divergences are unchanged in number
   and identity.

### What remains (why this issue stays open)

* **The entry locator.** The C TU dials `NROS_ENTRY_LOCATOR`, whose bare-metal
  bottom rung is `""`, so the backend substitutes its own default instead of the
  `[image.mps2-an385-baremetal] locator` in the leaf's `system.toml`. The leaf
  must NOT re-derive it — `<nros/entry_config.h>` is the one producer of that
  ladder (issue 0946) — so this is an entry-codegen job: something has to bake
  the resolved locator into the C compile for a cargo-rooted image. Without it
  there is no runtime lane, which is why the matrix tier would be `BuildOnly`.
* **No `[[fixture]]` row, hence no `matrix::CELLS` cell.** This would be the
  tree's first `lang = "c"` row wanting `builder = "cargo"`, and
  `scripts/build/fixtures-build.sh` still selects the CARGO lane with a
  `case "$lang" in c | cpp) ;;` proxy — the caller-side half of the same
  builder-vs-lang proxy phase-344 W2 fixed on the manifest side, where
  `row_builder()` already answers correctly. Fixing it in both directions plus
  adding the cell is its own change. The leaf is covered today by
  `just qemu build-examples`, whose loop globs every tracked
  `examples/mps2-an385-baremetal/**/Cargo.toml` and which `nightly.yml` reaches;
  it is listed in `examples_fixture_coverage.rs`'s `TEST_DRIVEN_BUILDERS` with
  that pairing recorded.
* **`cpp/` and the other five roles.** The `nros-cpp` staticlib builds for this
  target, so this is example-writing rather than wiring.
* **The C-ROOTED CMake road**, if anyone wants `nano_ros_add_executable` to work
  on a no-RTOS board: a `SECTIONS` linker script, a C reset/vector startup, and a
  C-callable board init — which needs a new staticlib crate for the board seam,
  because `nros-c` cannot depend on a board.
* **One bare-metal C limitation found by writing the leaf**: the `NROS_LOG_*`
  printf-style macros do not SUBSTITUTE here. `nros-baremetal-common`'s
  `vsnprintf` copies its format string verbatim (a deliberate choice — see its
  doc comment: an unsubstituted message still names the failure, where an empty
  buffer is silence), so `NROS_LOG_INFO(logger, "n=%d", n)` prints `n=%d`. The
  leaf builds its text and calls `nros_log_emit_at`. Whether the C log surface
  should offer a real bounded formatter on freestanding targets is a separate
  question nobody has posed.
