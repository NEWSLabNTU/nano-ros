---
rfc: 0077
title: "The image runtime is the image's choice"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: [issue-0618, issue-0617, issue-0615]
amends: [ARCHITECTURE.md#2-the-std-alloc-contract]  # §2 predates issue-0616; see "Where the design already is"
supersedes: []
superseded-by: null
---

# RFC-0077 — The image runtime is the image's choice

## Summary

`#[panic_handler]` and `#[global_allocator]` are link-time singletons of the
FINAL ARTIFACT. The tree has already accepted that for the ALLOCATOR — issue
0616 established it, and `check-archive-lang-items` now enforces one per link
line. **This RFC finishes the job for the PANIC HANDLER, which is the harder
half, and says why it is harder.**

Three concerns are collapsed into one rule today: the IMPLEMENTATION (how malloc
and panic work on this port), the INSTALLATION (which artifact carries the lang
item), and the POLICY (what a panic should actually do). The first is the
platform's and `ARCHITECTURE.md` §2 has it right. The second is the link root's
and 0616 settled it. The third belongs to the image and currently has no owner
at all — which is why a fixture that wants print-and-exit and a controller that
wants log-then-reboot get the same spin loop, and why issue 0617 has both an
image with two providers and an image with none.

The asymmetry is the crux. **The panic handler is a free choice** — every
behaviour is implementable everywhere, so keying it on the platform costs the
choice itself. **The allocator is not a choice, only a placement** — the image
decides where the static lives so there is exactly one, but what backs it must
stay the platform arena, because zenoh-pico's `z_malloc`, CycloneDDS's
`ddsrt_malloc` and the RTOS all allocate from the same heap and a second arena
would fragment memory nothing can measure.

nano-ros images also mix three languages, and the ABI is why they cannot agree:
`<nros/platform.h>` has **allocation** and no fatal path at all, so C and Rust
share one arena through `nros_platform_alloc` while each language invented its
own ending. This RFC adds the missing half — `nros_platform_panic` — so a Rust
panic, a C precondition failure and a C++ terminate converge on one symbol the
image controls. That is the same weak-default-the-application-overrides shape
Zephyr, FreeRTOS, NuttX and ESP-IDF all settled on independently.

Proposed, in one rule with three shapes: **the package that declares the entry
owns the image runtime** — the `*_entry` package in a workspace, the example
package itself when standalone, `nano_ros_entry()`'s generated TU for a C/C++
image. A panic raised anywhere in core, board, RMW or user code then reaches the
one handler that image declared, the way `examples/qemu-esp32-baremetal` already
works with `use esp_backtrace as _;`.

## Where the design already is — this completes it, it does not oppose it

An earlier draft of this RFC framed the problem as a disagreement with
`ARCHITECTURE.md` §2. That framing was wrong, and correcting it is most of the
argument.

§2 says:

> Orthogonally: **`malloc` and `panic` are unified per platform.** Exactly one
> `#[global_allocator]` and one `#[panic_handler]` per image, selected by the
> `platform-<rtos>` feature — which selects the provider and nothing else.

That text landed with phase-360 W1/W4 (`d56ed1fe3`) and **predates issue 0616**,
which then established the opposite of its unstated premise. 0616's own words:

> `#[global_allocator]` is a lang item: **unique per LINKED ARTIFACT**. nano-ros
> declares it in `nros-platform`, a mid-graph library, gated on a feature — and
> issue 0594's guarantee, "cargo unifies one crate's one feature into one unit",
> is a property of ONE graph. A staticlib is not a graph; it is a sealed copy of
> one. Four sealed copies can each contain the item and each be individually
> correct.

and its fix options are this RFC's design, written first and independently:

> **One link root per image, enforced** … those lang items belong to whoever owns
> the image, and a backend does not.
>
> **Move the item to the root crate** … the `#[global_allocator]` STATIC is
> installed by the link root through a macro. "One per image" then means "one
> root", which the build system already controls, rather than "one unit", which
> it does not.
>
> **A link-side gate** … `nm` the produced archives and assert at most one
> defines `___rust_alloc` per image.

The third has landed as `check-archive-lang-items` ("at most ONE Rust archive per
LINK LINE may define the global allocator"). So the tree has already moved to a
per-image model **for the allocator**. What has not moved is the panic handler,
and §2's text, which still describes the pre-0616 world.

## The distinction §2 conflates

Three separable concerns are collapsed into one sentence, and separating them
dissolves the apparent conflict:

| concern | question | belongs to | today |
| --- | --- | --- | --- |
| **implementation** | *how* does malloc/panic work here? | the PLATFORM | §2, correct |
| **installation** | which artifact carries the lang item? | the LINK ROOT | 0616, gate landed for alloc |
| **policy** | what should a panic DO? | the IMAGE | nobody |

§2's "selected by the `platform-<rtos>` feature" is right about
**implementation** — a platform genuinely does determine that `k_malloc` rather
than `pvPortMalloc` backs the heap. It is silent on **installation**, which is
what 0616 had to discover the hard way. And it has no place at all for
**policy**, which is the gap this RFC exists to fill.

## Why the allocator and the panic handler are not symmetric

This is the part neither §2 nor 0616 nor `nros-board-nuttx` separates, and it is
why panic is the harder half.

**The panic handler is a free choice.** Spin, halt, print-and-exit,
log-to-NVM-then-reboot are all implementable on every platform, and which is
right depends on what the image IS — a fixture whose harness greps the message,
a shipped controller, a bring-up image with a debugger attached. Nothing
constrains the image here beyond "exactly one".

**The allocator is not a choice at all — only a placement.** The image decides
WHERE the `#[global_allocator]` static lives (the link root, so there is exactly
one); it does not get to decide what backs it. The backing must stay the
platform arena, because the image's memory is not only Rust's:

- zenoh-pico allocates through `z_malloc` on the C side,
- CycloneDDS through `ddsrt_malloc` (CLAUDE.md: never libc — "RTOS heap is
  separate"),
- the RTOS itself through `k_malloc` / `pvPortMalloc` / `tx_byte_allocate`.

If the Rust side installed its own arena, an image would carry two heaps that
cannot see each other: fragmentation nothing can measure, and
`nros_platform_heap_used_bytes` — RFC-0034 D7's "true unified figure where the
platform owns one kernel heap shared by the C side and the Rust
`#[global_allocator]`" — silently stops being true.

The tree already builds it correctly, and says so:

> every `platform-*` feature resolves `ConcretePlatform` to `CffiPlatform`,
> whose `PlatformAlloc` impl IS `nros_platform_alloc`, and the bare-metal Rust
> crates reach their own arena through the same trait. **One API, one arena, per
> RFC-0034 D6.**

So the correction to make is narrow: move WHERE `PlatformGlobalAllocator` is
installed, never WHAT it forwards to. "Share the arena" is the invariant that
survives the move, and it is the reason the allocator gets an installation site
from the image and a policy from nobody.

## The evidence

### Six providers, five gating idioms

| provider | gate |
| --- | --- |
| `nros-c` spin loop (`src/lib.rs`) | `panic-spin` && !`std` && !`panic-halt` |
| `panic-halt` crate | `panic-halt` feature |
| `nros-board-nuttx` | `target_os = "nuttx"` && `image-runtime` |
| `nros-board-threadx-qemu-riscv64` | **ungated** |
| `nros-board-mps2-an385-freertos` | board owns it (issue #45) |
| libstd | whenever `std` is on |

### Three independent deciders, which must agree

1. `nros-c`'s `platform-*` features select `panic-spin`.
2. `cmake/NanoRosFeatureSet.cmake` appends `panic-halt` per platform tier.
3. Board crates carry their own, defaulted ON.

They are reconciled by a precedence rule written in prose (`panic-halt` beats
`panic-spin`, `std` supersedes both) and by consumers negating defaults.

### The composition rule lives in a doc comment

`nros-board-nuttx/src/lib.rs`:

> Exactly one `#[panic_handler]` may exist per image, and `nros-c` supplies one
> for `no_std` C/C++ images. Both crates are linked into a C/C++ NuttX image, so
> the two would be a duplicate-lang-item link error. Those images therefore take
> this crate with `default-features = false` and let `nros-c` own the image
> runtime; a pure-Rust image links no `nros-c` and takes this handler.

Correctness therefore depends on every consumer knowing which SHAPE of image it
is building. Nothing checks it.

## Why keying on the platform cannot work

The platform does not know the image's policy, and three images on one platform
legitimately want three different ones:

- a **test fixture** wants print-then-`exit(1)`, because the harness greps the
  message and the exit status;
- a **shipped controller** wants log-to-NVM-then-reboot;
- a **bring-up image** wants a spin loop, so a debugger can attach to a live
  core.

`nros-c` already concedes this in the source, while doing it anyway:

> A halt+reboot would be ideal but needs port-specific config … looping is the
> safest `no_std`-compatible default.

That is a library apologising for a policy only the image can choose. Issue 0594
already separated panic from the allocator because they are different facts;
this is the next step of the same correction — panic is not a *platform* fact
either.

## The existence proof

One platform already does this correctly, and it is the one where the upstream
ecosystem forced the question. `examples/qemu-esp32-baremetal/rust/talker`:

```toml
esp-backtrace = { version = "~0.18.0", features = ["esp32c3", "panic-handler", "println"] }
```

```rust
#![no_std]
use esp_backtrace as _;
nros::main!();
```

and `main_macro.rs` records the division of labour as a fact rather than a
workaround: *"The Entry crate provides the panic handler (`esp-backtrace`) and
app descriptor."*

So the proposed UX is not hypothetical and needs no new mechanism. It is what
one platform does today, what every embedded Rust project does, and what the
other platforms are prevented from doing.

## Three languages, one handler

nano-ros images mix C, C++ and Rust, and each language has its own idea of
"fatal". Today they do not meet:

| language | fatal path today | reaches |
| --- | --- | --- |
| Rust | `#[panic_handler]` | whichever library won the feature negotiation |
| C | nothing in the ABI — a port's own `assert`/`abort`, or nothing | the RTOS, or a silent spin |
| C++ | `static_assert` at compile time; no runtime fatal surface | — |

An image is one artifact, so one fatal path is the only coherent answer: a Rust
panic in the executor, a C precondition failure in a port, and a C++ terminate
must all end in the same place, because the operator debugging the board only
has one place to look.

**Four behaviours already exist, each hardcoded in a library**, which is the
same evidence from the other direction:

| provider | what it does |
| --- | --- |
| `nros-c` | `loop { spin_loop() }` — silent |
| `nros-board-nuttx` | `println!("nros: PANIC {info}")` then `exit(1)` |
| `nros-board-threadx-qemu-riscv64` | UART "PANIC: …" then exit QEMU |
| `nros-board-mps2-an385-freertos` | semihosting message, `bkpt #0`, then spin |

Those are four reasonable answers to four different questions ("is a human
watching?", "is a debugger attached?", "is a harness grepping stdout?"), which
is exactly why the choice cannot live in a library.

## The platform ABI has allocation and no fatal path

`<nros/platform.h>` exposes clock, **allocation**, atomics, sleep, yield,
random, wall clock, tasks, mutexes, condvars, wake, critical section and
logging. There is no section for "the world ended".

That asymmetry is the concrete defect behind the language split above. The
allocator is expressible as a platform fact — `nros_platform_alloc` — so C and
Rust already share one arena through it. Panic is not expressible at all, so
each language invented its own ending.

**Proposal — the ABI gains a fatal entry point, in the shape it already uses for
allocation:**

```c
/* ---- Fatal error ---- */

/** Terminate the image. Never returns.
 *
 *  `msg` is a diagnostic, NOT a C string: `len` bytes, no NUL required, and
 *  possibly empty. A port must tolerate being called from any context —
 *  interrupt, scheduler-locked, or before the kernel starts. */
_Noreturn void nros_platform_panic(const char *msg, size_t len);
```

Each port maps it to the native fatal path it already has, which is where the
per-RTOS knowledge belongs:

| port | native mapping |
| --- | --- |
| posix / threadx-linux | write to stderr, `abort()` |
| Zephyr | `k_panic()`, or `k_sys_fatal_error_handler` if the image installs one |
| NuttX | `PANIC()` / `up_assert`, `board_crashdump()` where configured |
| FreeRTOS | the existing hook body — message, `bkpt`, halt |
| ESP-IDF | `esp_system_abort()`, which honours `CONFIG_ESP_SYSTEM_PANIC_*` |
| bare-metal | message over the port's console, `bkpt`, then halt or reset |

**Rust then stops being special.** The entry's `#[panic_handler]` — the default
one nano-ros scaffolds — formats `PanicInfo` and calls `nros_platform_panic`. A
C caller reaches it directly. A C++ terminate handler forwards to it. All three
languages converge on one symbol, and the image decides what that symbol does.

## What real MCU and RTOS practice does

The proposal is deliberately unoriginal: every RTOS here already settled this,
and settled it the same way — **a weak default the application overrides**.

- **Zephyr** — `k_sys_fatal_error_handler(reason, esf)` is `__weak`; the default
  halts, and applications override it to log, reboot, or enter a safe state. The
  reason code distinguishes CPU exception, kernel oops, kernel panic and stack
  check failure.
- **FreeRTOS** — `configASSERT`, `vApplicationMallocFailedHook` and
  `vApplicationStackOverflowHook` are all application-supplied. nano-ros already
  implements two of them, in a board crate, hardcoded.
- **NuttX** — `PANIC()` routes to `up_assert()`, with `board_crashdump()` as the
  weak board hook for persisting state before reset.
- **ESP-IDF** — a panic handler that prints a backtrace, with the *policy*
  exposed as configuration: print-and-reboot, halt, or hand over to a GDB stub.

Two things are consistent across all four and belong in nano-ros's default:

1. **Say something before dying.** Every one of them emits a diagnostic first.
   A silent `loop {}` — which is `nros-c`'s current default, on the platform
   least likely to have a debugger attached — is the one behaviour none of them
   chose.
2. **Trap for the debugger, then halt or reset — deliberately.** `bkpt` on ARM
   when a debugger may be attached; reset on a shipped board, because a hung
   controller is usually worse than a restarted one. Which of halt or reset is
   right is a product decision, which is precisely why it is the image's.

ESP-IDF's shape is the closest fit for nano-ros: the *mechanism* is the
platform's, the *policy* is configuration, and the default is safe and loud.

## The proposed UX

### Which package configures it

One rule, three shapes: **the package that declares the entry owns the image
runtime**, because that package is what the final artifact is built from.

| image shape | the entry, and where the panic line goes |
| --- | --- |
| workspace | the `*_entry` package (`src/threadx_entry`, `src/esp32_entry`, …) — the one carrying `nros::main!()` |
| standalone example | the example package itself (`examples/<plat>/rust/<case>`), which carries `nros::main!()` directly |
| C/C++ image | `nano_ros_entry()`'s generated TU — the CMake analogue, where the staticlib IS the deliverable |

No other package says anything about it. A Node package, a board crate, an RMW
backend and `nros-core` are all libraries linked INTO the image, and none of
them claims the lang item.

### What that buys

Once the entry owns it, a panic raised **anywhere** — in `nros-core`'s executor,
in a board crate's bring-up, inside the RMW backend, in a user node — unwinds to
the one handler the image declared. That is the property the current design
cannot state: today the handler an image gets depends on which crate won the
feature negotiation, so the same panic in the same code reaches a spin loop in
one image and libstd's abort in another, for reasons the author never wrote down.

### What a user writes

The image's own crate declares its runtime, in one visible line:

```rust
#![no_std]
use panic_halt as _;        // or panic_semihosting, or esp_backtrace, or your own
nros::main!();
```

```toml
[dependencies]
panic-halt = "1"
```

A user who wants their own writes it, and nothing competes:

```rust
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    nvm::record(info);
    board::reboot()
}
```

### Configuring it, per language

The entry package owns it in all three, and each language uses its own native
override mechanism rather than a nano-ros invention:

**Rust entry** — a dependency and one `use`, which is what every embedded Rust
project already writes:

```rust
#![no_std]
use panic_halt as _;     // or esp_backtrace, panic_semihosting, panic_probe…
nros::main!();
```

Take nano-ros's default instead by writing nothing: the scaffolded entry carries
a handler that formats the message and calls `nros_platform_panic`, so the port
decides how to die and the image still gets the diagnostic.

**C / C++ entry** — define the symbol; the port's definition is weak:

```c
/* my_entry.c — overrides the port's default */
_Noreturn void nros_platform_panic(const char *msg, size_t len) {
    nvm_record(msg, len);
    board_reset();
}
```

**Neither, via configuration** — for the common cases, so a user does not write
code to pick a stock behaviour:

```cmake
nano_ros_entry(... PANIC halt)      # halt | reset | trap | custom
```

```toml
# the board's recommendation, which `nros new` writes into the entry
[board.image_runtime]
recommended_panic = "panic-halt"
```

**The allocator has no equivalent knob, deliberately.** There is nothing to
choose: the entry installs `PlatformGlobalAllocator`, it forwards to
`nros_platform_alloc`, and that is the arena the C side is already using. A
`PANIC`-style option for it would only offer ways to be wrong.

### What `nros new` scaffolds

The default must be **visible and editable**, not invisible and fought. Entry
scaffolding emits the two lines above, with the provider taken from the board
descriptor's recommendation (below). The user then sees, in their own crate, the
decision they are free to change — which is the difference between a default and
a constraint.

### What the board descriptor says

The board keeps its knowledge without keeping ownership. `nros-board.toml`
declares a RECOMMENDATION, consumed by scaffolding and by nothing at link time:

```toml
[board.image_runtime]
recommended_panic = "panic-halt"     # what `nros new` writes into the entry
```

Board knowledge stays in board data (RFC-0064's direction) while the binding
choice moves to the image.

## The design

Stated per layer, because the layers have different owners:

1. **Implementation stays platform-keyed.** `platform-<rtos>` continues to name
   the port that supplies malloc and panic mechanics. §2 is right here, and
   `check-platform-provider-features.py` (issue 0617) already enforces that
   every RTOS row names one — including that `platform-posix` must NOT, because
   libstd supplies both.

2. **Installation is the link root's, and stays coupled.** One artifact per
   image carries both lang items. This is `nros-board-nuttx`'s argument, kept
   intact: one owner, one switch, so no build can take one of each. What changes
   is direction — `image-runtime` stops being a default-ON feature on several
   crates that consumers must negate, and becomes a single positive statement
   made once per image. 0616's option (2) is the stronger form: `nros-platform`
   keeps providing the `GlobalAlloc` TYPE and the root installs the STATIC via
   `install_global_allocator!()`, so "one per image" means "one root" — which
   the build system controls — rather than "one unit", which it does not.

3. **Policy is the image's, and only for panic.** The entry package names what
   a panic does. The allocator gets no policy knob: the image chooses only the
   installation SITE, and what is installed stays `PlatformGlobalAllocator`
   forwarding to `nros_platform_alloc`. That constraint is not incidental — it
   is what keeps ONE arena shared with the C side (`z_malloc`, `ddsrt_malloc`,
   the RTOS's own), and what keeps `nros_platform_heap_used_bytes` a true
   figure rather than half of one.

4. **The entry layer materialises the default.** `nros::main!` and
   `nano_ros_entry()` generate code that IS part of the final artifact — the only
   place in nano-ros that can legitimately supply a default. A dependency cannot.

5. **The staticlib qualification.** `nros-c`/`nros-cpp` build
   `crate-type = ["staticlib"]`, and rustc treats a staticlib as a final
   artifact. When the staticlib IS the deliverable (a C/C++ image links it and
   nothing else) it needs a provider; when it is one input among several Rust
   crates it must not have one. That is what issue 0615 discovered, and it is
   knowable at the dep-site — the C/C++ build path knows it is producing the
   image. 0616 makes the same point from the other side: "a staticlib is not a
   graph; it is a sealed copy of one."

## The gate

The allocator half exists: `check-archive-lang-items` asserts at most one
archive per LINK LINE defines `__rust_alloc`. Two things are missing.

**The panic half.** The same script, the same link lines, `__rust_begin_short_backtrace`
being the wrong symbol to key on — `rust_begin_unwind` is the panic lang item's
external name and is what `nm` can see. Extending the existing check is a smaller
change than writing a new one.

**A coordinate-level view.** Per link line catches duplicates; it cannot catch
ABSENCE, because an image with no provider has no archive to count. Issue 0617's
`#[panic_handler] function required` was exactly that, and it is caught today
only by the build failing. Per buildable image coordinate, the count must be
exactly one — not at most one.

Two constraints, both learned here:

- Reason about **final artifacts**, not dep-sites — `check-feature-contract`
  clause (d) reasoned about dep-sites and asked for a provider a staticlib needs
  to be deleted (0615), and clause (e) counts source definitions when "the count
  it should be making is per produced archive" (0616's words).
- Run against **embedded** coordinates. `std` supplies both singletons, so the
  whole class is invisible on a host lane — 0617 records that NuttX's missing
  provider "was invisible for as long as NuttX images linked `std`".

## Migration

The six providers cannot move at once without a window where some image has
none. Ordering that keeps the tree green:

1. Add the gate first, reporting only. It names today's true owner per
   coordinate, which is the inventory this RFC could not otherwise trust.
2. Add the positive `image-runtime` selection and have the entry layer set it,
   with the existing defaults still ON — no image changes owner yet.
3. Flip defaults OFF one provider at a time, gate enforcing, starting with the
   ungated `nros-board-threadx-qemu-riscv64` because it is the one that cannot
   currently be turned off at all.
4. Scaffold the user-visible lines; update the book's embedded pages, where the
   panic decision is currently never mentioned because it never had to be.

## Alternatives considered

**Keep platform-keying, add a conflict gate.** Cheaper, and it would catch 0617.
But it leaves the user unable to choose a handler at all — the defect a fixture
that wants print-and-exit hits today — so it fixes the symptom and keeps the
cause.

**One nano-ros-owned handler for every image.** Simplest to reason about, and
wrong for the same reason: it is a policy, and policies belong to the image.
It also cannot serve ESP32, whose ecosystem supplies its own.

**Split panic and allocator into separate image flags.** An earlier draft
rejected this, citing `nros-board-nuttx`: "two flags would let a build take one
of each and duplicate a lang item." That rejection was wrong, and the error is
instructive — it applies an INSTALLATION argument to a POLICY question. The
board's reasoning is sound about installation: one owner should install both,
or two crates can each install one. It says nothing about who chooses what a
panic does. So the answer is to couple INSTALLATION (one owner, one switch,
exactly as the board argues) and decouple POLICY (the image names the panic
behaviour; the allocator has only one sensible answer and stays with the
platform).


## Review feedback 2026-08-16 — two gaps in the Rust-entry framing

Raised by the maintainer against the W5 surface. Both are about the same
assumption: that "the image" is always a Rust entry crate that can write a line.

### 1. A mandatory `panic_to_platform!()` line is not the Rust convention

In `no_std` Rust an image does not normally *write* a framework line to get a
panic handler. It either pulls one in as a dependency (`use panic_halt as _;`,
`use esp_backtrace as _;`) or writes `#[panic_handler]` itself. Requiring
`nros::panic_to_platform!()` beside `nros::main!()` adds a second obligatory
line that no other `no_std` crate asks for, and the failure mode when it is
forgotten is a link error about a missing lang item rather than anything that
names this framework.

The RFC's stated reason for keeping it separate — "emitting one silently from
`nros::main!()` would collide with every image that already declares its own" —
holds only for an UNCONDITIONAL emit. `main!()` is already a macro that takes
arguments, so the default can be emitted and suppressed explicitly:

```rust
nros::main!();                     // default: panics route to nros_platform_panic
nros::main!(panic = "own");        // this image declares its own handler
```

The opt-out keeps what the RFC actually cares about — the image says what it
wants, once, visibly — while removing the line every image has to remember.
`esp-backtrace` and `panic-semihosting` images write the opt-out; they are still
right, and they are still the ones who said so.

**A constraint any variant must respect:** Rust has no weak or overridable lang
item. "Default that the image overrides" cannot be expressed at link time — two
`#[panic_handler]`s is a compile error, not an override. So a default has to be
suppressible AT THE EMIT SITE (a macro argument or a cargo feature) and can
never be a fallback the linker discards.

### 2. A C/C++ entry package cannot write any of these lines

`nros-c` and `nros-cpp` are `crate-type = ["staticlib", …]`, and
`nros-cpp/Cargo.toml` already records the consequence:

> its `[lib] crate-type` includes `staticlib`/`cdylib`, so rustc emits those
> final artifacts for the dep too; **on a no_std target each needs a
> `#[panic_handler]`**

rustc requires the lang item WHEN THE STATICLIB IS COMPILED. By the time cmake
links the C/C++ executable, that decision is already made and baked into
`libnros_c.a`. A C or C++ entry therefore cannot supply a Rust lang item at all —
there is no Rust crate in the image for it to live in, and the link root is not
a Rust compilation unit.

So W5.e's plan — "`panic-halt` stops being a library feature and becomes what it
always should have been, a dependency the IMAGE names" — is a Rust-entry answer.
On the C/C++ path there is nothing to name it. Today the choice is made by
`cmake/NanoRosFeatureSet.cmake`, which hardcodes it per platform:

```cmake
list(APPEND _feats alloc panic-halt platform-freertos)     # :120
list(APPEND _feats alloc panic-halt platform-threadx)      # :126, :140
list(APPEND _feats alloc panic-halt "platform-${_FS_PLATFORM}")  # :147
```

That is the same defect this RFC exists to fix, one language over: a LIBRARY
decision, made on the image's behalf, by a table the image's author never sees
and has no documented way to override.

**What the C/C++ path needs is the same shape expressed in its own vocabulary.**
The policy still belongs to the image; the image is a cmake target rather than a
Rust crate, so the knob belongs on `nano_ros_entry()` and lowers to the cargo
feature the staticlib is built with — e.g. `nano_ros_entry(… PANIC platform|halt|own)`,
defaulting to the platform route and requiring `own` before the user supplies a
strong symbol themselves. The invariant is unchanged; only the surface differs.

Worth stating plainly in §2's amendment (W5.f): **the image owns the panic
policy, and how it says so depends on what the image is written in.** A Rust
entry says it in a macro; a C/C++ entry says it in the cmake call that builds it.
Leaving the second unaddressed would close this RFC with the C/C++ half of the
tree still in exactly the state the RFC describes as wrong.


## Decision 2026-08-17 — one policy, two surfaces

The review above is accepted. The invariant does not change: **the image owns the
panic policy, and exactly one provider reaches the artifact.** What changes is
that the image says so in the vocabulary it is written in, and that saying
nothing gets a working default instead of a link error.

### The surfaces

| the image is | says it with | default |
| --- | --- | --- |
| a Rust entry crate | `nros::main!(panic = …)` | `platform` |
| a C/C++ cmake target | `nano_ros_entry(… PANIC …)` | `platform` |

Both accept the same three values, and they mean the same thing on either side:

| value | meaning |
| --- | --- |
| `platform` | route panics to `nros_platform_panic` — the board's honest ending |
| `halt` | the `panic-halt` body: park the core, for images that must not print |
| `own` | this image supplies its own provider; emit nothing |

`own` is a positive declaration, not the absence of one. That is the whole point
of the opt-out: an image that brings `esp-backtrace` or `panic-semihosting`
STATES it, so the build can tell "deliberate" from "forgot" — which is the
distinction the current design cannot make.

### Amendment 2026-08-18 (b) — the question is WHO LINKS THE FINAL IMAGE

The three values and the placement rule both answer a prior question that this
RFC never stated, and stating it explains the awkward cases instead of listing
them.

**rustc's notion of a final artifact is not the system's.** rustc demands the
lang item whenever it emits a `staticlib`. But on Zephyr, on the ThreadX CMake
path, and for every C/C++ image, that archive is an INPUT to a link step some
other build system owns — the real image is an ELF produced by west or CMake. So
`crate-type = ["staticlib"]` imposes a rustc-level obligation on a crate that is
emphatically not the image.

This is the tension, and it is why "does this crate need a handler?" has no
answer at the crate level. The answerable question is:

> **Who links the final image?**

| who links it | the ending belongs to | in this tree |
| --- | --- | --- |
| cargo | our entry package | freertos, nuttx, arm-baremetal bins — `nros::main!(panic = …)` |
| another build system, which brings its own runtime | that runtime | Zephyr: `zephyr-lang-rust` links our `rustapp` into its ELF and the `zephyr` crate supplies the handler |
| another build system, with no runtime of its own | our staticlib, because nothing else will | a C/C++ image linking `libnros_c.a`; ThreadX-RV64's CMake path |

**Zephyr is not the exception — it is the second row, and it says so in the
source.** `examples/workspaces/rust/src/zephyr_entry/src/lib.rs`: *"Zephyr's
allocator + panic + boot belong to the RTOS."* That entry package supplies no
provider and needs none. A Zephyr entry that said `panic = "platform"` would
collide with the `zephyr` crate's handler — W5.b's duplicate-lang-item failure
arriving from the other direction.

**The workspace case is the first row, and the tree already matches.** Every
`*_pkg` node package under `examples/workspaces/rust/src/` carries zero
providers; only entry packages do. A node is a link in an image whose shape the
entry defines, so a node must never embed an ending — it cannot know which image
it will end up in, and two nodes in one image would be a duplicate.

**Corrects the wording of amendment (a), not its code.** "The package produces a
staticlib, therefore the LIB is the image" is the right derivation with the wrong
reason. The truth is "rustc will demand an item where it emits an archive,
whether or not that archive is the image" — which is also why `PANIC own` must be
sayable on the C/C++ side for a Zephyr-hosted target: the RTOS has it covered.

**And it decides M4's open question.** The per-platform table in
`NanoRosFeatureSet.cmake` cannot answer "does anything else supply the handler?",
because that is a property of the ENTRY and its link step, not of the platform.
Only the entry knows. So the staticlib's feature set has to be computable after
entries are declared, rather than fixed at import time from globals — the
deferred option, not the pre-import global.

### Amendment 2026-08-18 — the policy is declared, the PLACEMENT is derived

Implementing M1 surfaced a third case the two-surface table does not cover, and
resolving it removes a user-facing choice rather than adding one.

**The case.** `nros::main!()` expands in `main.rs`, the bin target. Six examples
— `examples/qemu-riscv64-threadx/rust/*` — are `crate-type = ["staticlib",
"rlib"]` and produce TWO final artifacts from one crate: a bin (cargo/zenoh) and
a `.a` (CMake/CycloneDDS, whose C `startup.c::main` dispatches to `app_main`).
rustc demands the lang item when it compiles the staticlib, which is built from
`lib.rs`'s module tree, so a handler emitted in `main.rs` never reaches the `.a`,
while one in the lib reaches BOTH — the bin links the rlib and inherits it.

**The wrong fix, recorded because it is the obvious one.** Declare those six
`panic = "own"` and leave their `panic_to_platform!()` where it is. It works, and
it destroys the only thing `own` is for: `own` would then mean either "I bring my
own provider" or "my provider lives in the other artifact's macro", and the gate
could no longer tell deliberate from forgot — the distinction this whole decision
exists to create. It also asks the author of a talker example to know a Rust
linkage rule in order to answer a question about panics.

**The rule.** One sentence, covering every family:

> The entry macro of a final artifact emits that artifact's handler.

Each artifact already has exactly one entry macro, and after phase-366 W7's
rename none of them is named for an RMW:

| artifact | entry macro | families |
| --- | --- | --- |
| bin (cargo) | `nros::main!()` | native, freertos, nuttx, arm-baremetal, ThreadX-RV64 zenoh |
| `.a` (CMake) | `<board>::app_main!()` | ThreadX-RV64 cyclone |
| `.a` (west) | `nros::zephyr_component_main!()` | zephyr |
| `.a` (C/C++ image) | `nano_ros_entry(… PANIC …)` → one cargo feature | nros-c / nros-cpp |

**How the dual-artifact crate avoids emitting twice.** `main!()` already parses
the entry's `Cargo.toml` (three sites in `main_macro.rs`). It reads `[lib]
crate-type`: if the package produces a `staticlib`, the lib owns the item for
both artifacts and `main!()` emits nothing. Derived from the manifest, never
chosen by the author.

**Hosted images are excluded structurally.** The emit is gated
`#[cfg(target_os = "none")]`, the same condition `main!()` already uses to tell a
bare-metal image from a hosted one. libstd defines the lang item, so without this
gate M5's default flip would break every native example at once.

**Two adjacent facts, established while implementing this and recorded so they
are not re-derived.** `nros_board_threadx_qemu_riscv64::cyclonedds_app_main!()`
was renamed `app_main!()`: its body is `run_app_thread($register)` and nothing in
it is CycloneDDS — Cyclone is merely the one backend whose embedded build must be
CMake-linked, which is a BUILD-SYSTEM property. An entry macro named for a
backend contradicts the RMW-portability promise; the divergence that produced the
name is issue 0666. And `examples/threadx-linux/rust/*` declared
`crate-type = ["rlib", "staticlib"]` with nothing consuming the `.a` — dropped,
same removal and reasoning as phase-359 W7 on qemu-arm-nuttx. A `staticlib` is a
final artifact and carries a lang-item obligation; declaring one nothing builds
is an obligation with no consumer, invisible there only because the family is
hosted and libstd satisfied it.

### Why a default is safe here, given there are no weak lang items

Rust has no overridable `#[panic_handler]`; two definitions is a compile error.
So this default is NOT a link-time fallback. It is a macro/feature-level
decision resolved before rustc runs:

- the Rust entry's default is resolved when `main!()` expands;
- the C/C++ entry's default is resolved when cmake computes the staticlib's
  cargo features.

In both cases exactly one provider is chosen before compilation, and `own`
suppresses emission at that same point. Nothing is discarded by the linker, and
the "default versus override" shape that Rust cannot express is never relied on.

### What this replaces on the C/C++ side

`panic-spin` was never a policy anyone chose — it was the body `nros-c` happened
to carry, and `cmake/NanoRosFeatureSet.cmake` hardcoded `panic-halt` per platform
beside it. Both are replaced by ONE feature the entry selects:

    nros-c / nros-cpp features
      panic-platform   emits #[panic_handler] forwarding to nros_platform_panic
      panic-halt       existing panic-halt body
      (neither)        PANIC own — the image supplies the provider

Exactly one of the two features is enabled for a `no_std` staticlib build, and
that is checkable rather than conventional (see the gate below).

**Note the behaviour change this carries.** Today a C/C++ embedded image halts on
panic, because the table says `panic-halt`. Under `PANIC platform` it will route
to `nros_platform_panic` and end the way the board ends — printing on ports that
print, `k_panic()` on Zephyr, exiting QEMU on the ThreadX RV64 board. That is the
intended ending, but it is a change in what a shipped image does, so it belongs
in the migration notes rather than in a silent default flip.

## Migration

Ordered so that no commit leaves an image with two providers or none. Each step
is independently green.

**M1 — add the argument, default `own`.** `main!()` accepts `panic = …` and, for
now, defaults to `own` (emit nothing). Identical behaviour to today; no image
changes. The C/C++ `PANIC` argument lands the same way, defaulting to whatever
`NanoRosFeatureSet.cmake` currently computes for that platform.

**M2 — migrate the Rust images that already opted in.** The ~23 images calling
`nros::panic_to_platform!()` convert, one commit per family:

```rust
-nros::panic_to_platform!();
-nros::main!();
+nros::main!(panic = "platform");
```

Removing the call and adding the argument in the same edit is what keeps each
commit self-consistent — do them separately and the image has two providers or
none in between.

**M3 — declare the images that bring their own.** `qemu-esp32-baremetal`
(`esp-backtrace`), `logging-smoke-freertos-mps2` and
`examples/workspaces/rust/src/freertos_entry` (`panic-semihosting`) gain
`panic = "own"`. Behaviour unchanged; they now SAY what was previously inferred
from their silence. This step must complete before M5.

**M4 — migrate the C/C++ entries.** Each `nano_ros_entry()` that wants the
board's ending gains `PANIC platform`; any image depending on halt semantics
says `PANIC halt` explicitly. The per-platform table stops being consulted for
this decision.

**M5 — flip both defaults to `platform`.** Only now, with every image either
migrated or explicitly `own`. An image added after this point gets a working
ending by saying nothing, which is the ergonomic goal.

**M6 — extend the gate to ABSENCE.** `check-archive-lang-items` counts per link
line, which catches duplication and cannot catch a missing handler. Counting per
image COORDINATE lands here — phase-366 already names this as a prerequisite
rather than a nicety, and M7 is what makes it load-bearing.

## Retirement

**R1 — `panic-spin` is deleted** from `nros-c` and `nros-cpp`, along with the
`#[panic_handler]` at `nros-c/src/lib.rs:160`. Blocked on M4: it is the only
provider a C/C++ `no_std` staticlib has today, so deleting it before the entry
can name a replacement leaves the archive with no lang item — the absence M6
exists to catch.

**R2 — `panic-halt` stops being a per-platform default.** The hardcoded
`panic-halt` entries in `cmake/NanoRosFeatureSet.cmake` (lines 120, 126, 140,
147) are removed; the feature remains, selectable as `PANIC halt`.

**R3 — `panic_to_platform!()` stays, and stops being the documented path.** It
remains public for entries that do not go through `main!()` (a hand-rolled
`no_std` binary, `zephyr_component_main!`), and its doc comment changes from
"invoke this" to "`main!(panic = …)` is the normal way; this is the escape
hatch". Not deleted — deleting it would strand exactly the images that cannot
use the macro that replaces it.

**R4 — §2's sentence is amended** (phase-366 W5.f), and now says which surface
carries the choice, because "the image" is not always a Rust crate.

## Acceptance

- A new Rust entry that writes only `nros::main!()` links and panics through
  `nros_platform_panic`.
- A new C/C++ entry that writes only `nano_ros_entry(…)` does the same.
- An image that supplies its own provider and forgets to say `own` FAILS, and
  the message names `panic = "own"` / `PANIC own` rather than the lang item.
- An image that says `own` and supplies nothing FAILS with a missing provider,
  from the coordinate-level gate rather than from the linker.
- `grep -rn "panic-spin" packages/ cmake/` is empty.
