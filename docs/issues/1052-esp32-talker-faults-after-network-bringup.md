---
id: 1052
title: "The esp32-qemu talker takes an instruction-access fault right after network bring-up, with a return address made of ASCII"
status: open
area: rmw, boards
severity: high
found: 2026-09-04
related: [0968, 1048, 0291]
---

# The PC is a string, not a function

## What the image prints

Run alone under QEMU with a zenoh router up:

```
Initializing OpenETH...
  IP: 10.0.2.51/24
Ethernet ready.

Exception 'Instruction access fault' mepc=0x732f7264, mtval=0x732f7264
TrapFrame { ra: 1932489317, t0: 9, t1: 0, t2: 1070320272, … }

Backtrace:
0x42051d70
```

It never reaches `Application setup complete`, so `register()` — one publisher,
one timer — does not finish.

## The registers are ASCII, which is the whole lead

```
mepc = mtval = 0x732f7264  ->  b"s/rd"
ra              0x732f7265  ->  b"s/re"
```

Execution branched to an address whose bytes are printable source-path text, and
`ra` holds the adjacent value. Strings of that shape in this binary are
`file!()` paths in rodata — `zenoh-pico/src/collections/refcount.c` and
`src/iter/adapters/rev.rs` both contain the `s/re` run.

So this is a corrupted return address or function pointer, not a missing symbol
or an unmapped fetch: something wrote text over a code pointer, or a pointer was
read from the wrong place. `refcount.c` is zenoh-pico's reference counting,
which is a plausible neighbourhood for it and is NOT a claim that it is the
culprit.

## Not caused by the logging fix, and not new

The same `Instruction access fault` appears twice in the tier-2 capture from
BEFORE issue 1048's fix landed
(`test_esp32_to_native`, run at the original tree state). The fix changed what
the board can print; it did not change this.

**It was invisible for the same reason everything else was.** With no log sink
installed (issue 1048) the image could not say where it stopped, so this read as
"the talker never printed `Publishing:`" — a missing marker rather than a crash.
The two failures were stacked, and the logging one had to come off first.

## The contrast that scopes it

The LISTENER, same board, same build, same run conditions, does not fault. It
reaches `Application setup complete`, enters its spin loop, and then reports
`zpico Generic -> ConnectionFailed` repeatedly — a different problem, and one it
survives.

So this is specific to the talker's path. The difference in `register()` is a
publisher plus a timer that `publishes_entity` against it, where the listener
creates one subscription.

## What this blocks

`test_esp32_talker_listener_e2e` and `test_esp32_to_native` in
[issue 0968](0968-tier2-runtime-failures-unreproduced.md). Both wait on the
talker's `Publishing:` marker, which the image cannot reach.

## BISECTED 2026-09-04 — it is NOT in `register()`

Six images, one variable each, every one built with the row's env and run under
QEMU with a router up. `setup_complete` is `Application setup complete`;
`fault` is the instruction-access fault.

| variant | setup_complete | fault |
| --- | ---: | ---: |
| A — `create_node` only | 0 | 1 |
| B — + publisher | 0 | 1 |
| C — + timer | 0 | 1 |
| D — full (`publishes_entity`) | 0 | 1 |
| **E — `register` does NOTHING (`Ok(())`)** | **0** | **1** |
| F — E, plus `ENTITY_BOUNDS::exact(0,0,0,0,0)` | 0 | 1 |
| G — full, but with the LISTENER's `ip = 10.0.2.51` | 0 | 1 |

**The fault address is `0x42051d70` in every one.**

So it is none of: the publisher, the timer, `publishes_entity`, the entity-bounds
static, or the static IP. An EMPTY `register` faults identically, which puts it
outside the node's own registration code entirely — in the image's startup path,
before or around `Executor::open`.

### Cut further, 2026-09-04 — and the CONTROL corrected the framing

| variant | setup_complete | fault |
| --- | ---: | ---: |
| H — empty `register` AND empty `on_callback` | 0 | 1 |
| I — full talker, node `name = "listener"` | 0 | 1 |
| **listener, UNMODIFIED** | **1** | **0** |
| **listener, EMPTY `register`** | **1** | **0** |

The listener control is the one that matters. An empty `register` does NOT cause
the fault: the listener with the same empty body completes setup and spins. So
"an image that registers no entities faults" is REFUTED, and the earlier
contrast — which compared a non-empty listener against an empty talker — was not
like-for-like.

**Ruled out for the talker, each by a single-variable image:**

* the whole body of `register` (A–E)
* the `ENTITY_BOUNDS` static (F)
* the static IP (G)
* the node type's `on_callback` body (H)
* the node NAME (I)
* "no entities" as the trigger (the listener control)

Same fault address `0x42051d70` throughout.

### And yet the listener does not fault

Same board, same build system, same run conditions, a NON-empty `register`, and
it reaches its spin loop. After G, the differences left between the two leaves
are down to the node NAME (`"talker"` vs `"listener"`) and the node TYPE itself —
`Talker`'s `ExecutableNode` impl, its `State = i32`, its `on_callback`. Those are
what a further bisect should cut.

## Memory pressure REFUTED, and the constant PC is the finding (2026-09-04)

The leaf does not link without `ZPICO_MAX_QUERYABLES=2`, so "the image is over
budget and smashes its stack" was the obvious next hypothesis. It is wrong:

| `ZPICO_MAX_QUERYABLES` | fault | `mepc` |
| --- | ---: | --- |
| 1 | 1 | `0x732f7264` |
| 2 (shipped) | 1 | `0x732f7264` |
| 4 | 1 | `0x732f7264` |

Three builds with different session-struct sizes and therefore different
layouts, and **the faulting PC is byte-identical in all three**.

That rules out stack smashing, which would move with layout, and it rules out
memory pressure as the trigger. A constant wrong PC across differing builds
means the value is DATA the code reads deterministically — something loads
`0x732f7264` ("s/rd", printable text) from a fixed place and calls it.

So the shape is: **a code pointer read from a slot that holds string data**, not
random corruption. Candidates worth looking at first are the fn-pointer slots
this board actually installs — `nros_platform_esp32_qemu::register_log_writer`'s
writer slot, and the RMW backend registration
(`nros_rmw_zenoh::register()`) — because those are the places a `fn` value is
stored and later called on this target. That is a direction, NOT a diagnosis.

## Where to start

1. Resolve `0x42051d70` (the backtrace frame, which IS a code address unlike
   `mepc`) against `esp32_qemu_talker` with a RISC-V `addr2line`. The host
   `addr2line` in this checkout does not read it; the toolchain's
   `llvm-addr2line` should. This is now the single highest-value step: it names
   the caller that loaded the bad pointer.
2. Find what stores `0x732f7264` — search the ELF for that byte sequence and see
   which object it lands in. The PC is constant across builds, so the source is
   too.
3. ~~Bisect `register()`~~ — done, and it is none of it (table above).
4. ~~Suspect memory pressure~~ — refuted above.
   `llvm-addr2line` should.
2. Bisect `register()`: publisher only, then timer only. The fault is before
   `Application setup complete`, so it is inside those three calls.
3. Note that zenoh-pico is pinned 1.7.2 (issue 0291) and the esp32 image is the
   RAM-tightest in the tree — a corrupted pointer here may be an overflow of
   something sized by a knob rather than a logic bug.


## Resolved with the RISC-V toolchain (2026-09-04) — and the ASCII lead is WEAKENED

`riscv32-esp-elf-addr2line` (esp-13.2.0, in `~/.espressif`) on the talker ELF.

**`0x42051d70`, the only backtrace frame:**

```
.L0
esp-hal-1.0.0/src/exception_handler/mod.rs:92
```

That is the exception handler ITSELF. There is no frame beneath it, because
`ra` was corrupted along with `pc` — so the backtrace cannot name the caller,
and step 1 of the previous plan is exhausted rather than pending.

**`0x732f7264` is not a constant in the image.** Searched the whole file:

| pattern | occurrences |
| --- | ---: |
| bytes `64 72 2f 73` (the value, little-endian) | 0 |
| the text `s/rd` | 0 |
| the text `dr/s` | 0 |

So the value is ASSEMBLED AT RUNTIME, not loaded from a stored pointer or a
literal. It also resolves to nothing (`??:0`) and lies outside every `LOAD`
segment — the image maps `0x4038xxxx`, `0x3fc8xxxx` and `0x3c00xxxx`, nowhere
near `0x732f7264`.

### Correcting my own lead

This issue opened by calling the ASCII reading "the whole lead". That was
overstated, and the check above is what shows it: nothing in the image contains
those bytes in either order. All four bytes of `0x732f7264` land in printable
ASCII, but that happens by chance about 1.9% of the time, and one 4-byte
coincidence is not evidence of a string. **Treat "the PC is a string" as
unproven.**

What survives from the earlier work is the stronger, measured fact: the value is
IDENTICAL across three builds with different layouts, so whatever produces it is
deterministic — but it is computed, not fetched from a constant.

### What would actually move this

* **A hardware watchpoint / single-step under `riscv32-esp-elf-gdb`** (also
  installed here, in `~/.espressif/tools/riscv32-esp-elf-gdb`), attached to
  QEMU's gdbstub. That is the instrument this needs: it can stop at the faulting
  instruction with the register file intact and show what computed the value,
  which static inspection cannot.
* Everything reachable by reading the image or bisecting the source has now been
  tried: seven single-variable images, a control on the other leaf, three
  memory-pressure points, and both address lookups.

## The fault is on the CONNECTED path (2026-09-04)

Found by accident while setting up gdb, and it is the sharpest cut yet.

Under the gdbstub the talker did NOT fault. It printed:

```
[ERROR] nros: zpico Session -> ConnectionFailed
Executor::open failed: Transport(ConnectionFailed)
```

The reason was environmental — the router had failed to start with
`libzenohc.so: cannot open shared object file`, which is
[issue 0774](0774-*) exactly (`rmw_zenohd` resolves but does not run without the
paired library on `LD_LIBRARY_PATH`). But the accident is the datum:

| router | session | fault |
| --- | --- | ---: |
| up (every earlier run) | connected — `ConnectionFailed` count 0 | **1** |
| down (the gdb run) | `ConnectionFailed` | **0** |

Every run that faulted has ZERO `ConnectionFailed` lines; the run that could not
connect did not fault. **The talker reaches the fault only when its zenoh
session establishes.** With no peer it fails `Executor::open` cleanly and stops,
like any node would.

### Why that matters for the earlier cuts

The variants with an EMPTY `register` still faulted — and now we know they also
connected. So the fault is in the post-connect path, reached before
`Application setup complete`, and it does not need a publisher, a timer or a
callback to exist. That is consistent with every cut so far and narrows where to
look: between session establishment and the return of the run-plan closure.

It also explains the listener contrast better than "the listener is fine": in the
control runs the listener logged `ConnectionFailed` repeatedly, i.e. it was on
the UNCONNECTED path throughout. **The two leaves have not yet been compared with
both connected**, and that comparison is now the first thing to do — it may show
the listener faults too, which would move this off "talker-specific" entirely.

### Consequence for anyone reproducing

Start the router and CONFIRM it is serving before drawing any conclusion from an
esp32 run. `just esp32 zenohd` can exit 127 on the libzenohc pairing and leave
you measuring the unconnected path, which looks like "no fault" and is not.

**Note on "Where to start" #1 above:** the static analysis below shows that
resolving `0x42051d70` cannot name the caller — it is a frame of the panic
printer, not of the faulting code. Step 1 is a dead end by construction; steps 2
and 3 are unaffected.

## gdb names the frame (2026-09-04): `ZenohSession::create_publisher`

`riscv32-esp-elf-gdb` against QEMU's gdbstub, with the router CONFIRMED serving
on :9800 first. Break at the handler, dump the stack, resolve every code address
on it:

```
0x42017ec8  <nros_rmw_zenoh::shim::session::ZenohSession as nros_rmw::traits::Session>::create_publisher
0x420531ca  esp_hal::interrupt::riscv::vectored::enable_on_cpu
0x4205334c  <riscv_rt::TrapFrame as core::fmt::Debug>::fmt
0x4205e950  <i32 as core::fmt::Display>::fmt
```

Everything but the first is the exception handler printing itself. **The only
application frame on the faulting stack is `ZenohSession::create_publisher`.**

At the handler: `sp` is in `.bss` near `nros_smoltcp::TCP_RX_BUFFER_0`, and `ra`
resolves to `core::panicking::assert_failed_inner+2` — a mid-symbol offset, so
treat that as nearest-preceding resolution and NOT as proof of a panic frame.
The `a0`/`a2` strings are the handler's own format literals
(`"Exception '...' mepc=0x..."`), not an assertion message; an earlier reading of
them as one would have been wrong.

### It fits the leaf split — and CONTRADICTS the empty-register result

A publisher is what the talker creates and the listener does not, so the frame
explains the split immediately. But variants E, F and H registered NOTHING and
faulted identically, and with an empty `register` no publisher is created.

Both cannot be true. Either:

* the empty-`register` images were not actually rebuilt before packing — the
  pack step may have used a cached ELF, which would make E/F/H measurements of
  the ORIGINAL image; or
* something creates a publisher during session setup regardless of what the node
  registers (liveliness, the graph, an internal token), in which case the frame
  is reached on every image and the leaf split has a different cause.

**Resolve that before acting on either.** The check is cheap: rebuild an
empty-`register` talker, confirm the string `Hello World` is ABSENT from the ELF
(it only exists in the publishing path), and re-run. If it faults with the
publisher genuinely gone, the second explanation holds.

This is the same class as the four retracted issues 0859-0862: a measurement
whose artifact provenance was never checked. My E/F/H runs did not verify that
the packed image contained the edit.

## STATIC ANALYSIS 2026-09-04 — the registers say it is a RETURN, and the backtrace frame is a ghost

No build was run for this section. Everything below is either arithmetic on the
register values already in this issue, source read at `origin/main`, or
read-only `nm`/`readelf`/`objdump`/`strings` against the **only esp32 ELF on
this host**, which is

```
build/cargo-fixtures/qemu-esp32-baremetal/riscv32imc-unknown-none-elf/
  nros-relwithdebinfo/esp32_qemu_{talker,listener}     built 2026-08-15 11:35
```

That is 20 days old and predates BOTH the 1048 fix (6d9b0722, 2026-09-04) and
the exact-`ENTITY_BOUNDS` commit (6ae0249a, 2026-08-28). It is **not** the
binary that produced the trace above. Every address resolved against it is
labelled as such; the register arithmetic and the source reads do not depend on
it.

### 1. MEASURED — the faulting instruction was `ret`, so it is NOT a call through a bad fn pointer

```
mepc = mtval = 0x732f7264
ra           = 0x732f7265
ra & ~1      = 0x732f7264   == mepc, exactly
t0           = 9            (so the jump did not go through t0)
```

In RISC-V, `jalr rd, rs1, imm` computes `target = (rs1 + imm) & ~1` and writes
`rd = pc + 4` (`pc + 2` for `c.jalr`). An instruction-access fault on the target
is raised on the FETCH, after the jump has retired — so `rd` is already written.

* If this had been an indirect **call** (`jalr ra, 0(rs)` / `c.jalr rs`, which
  is what a fn-pointer slot compiles to) then `ra` would hold a valid `.text`
  return address. It holds printable ASCII instead.
* `target == ra & ~1` with `ra` unchanged is the signature of
  `ret` = `jalr x0, 0(ra)` (`c.jr ra`): `rd = x0`, so `ra` survives, and the
  hardware clears bit 0 of the target. All three reported values follow from
  that one instruction.
* A tail call `jr t0` would need `t0 == mepc`; `t0` is 9.

**So the previous section's lead is refuted by its own numbers.** The fn-pointer
slots named there — `register_log_writer`'s writer slot, `nros_rmw_zenoh::register()` —
are *called*, and a call writes `ra`. What happened here is that a function
**returned to a return address that had been overwritten**.

### 2. MEASURED — the byte order in this issue is backwards, which changes the candidate strings

`ra` is restored with `lw ra, off(sp)`, a LITTLE-endian load. The bytes actually
sitting in that slot are therefore

```
0x732f7265 -> 65 72 2f 73 -> "er/s"        (not "s/re")
```

`"s/re"` is the big-endian reading and matches nothing that is loaded that way.
So `refcount.c` and `adapters/rev.rs` are **not** candidates; strings containing
`er/s` are. Examples that exist in this tree:

* `…/examples/qemu-esp32-baremetal/rust/talk` **`er/s`** `rc/lib.rs`
* `…/examples/qemu-esp32-baremetal/rust/listen` **`er/s`** `rc/lib.rs`
* `…/esp-hal-1.0.0/src/tim` **`er/s`** `ystimer.rs`
* a runtime-built rmw_zenoh keyexpr, `0/chatt` **`er/s`** `td_msgs::msg::dds_::String_/…`

Relevant here: `nros_log`'s macros capture `::core::file!()`
(`packages/core/nros-log/src/macros.rs:35`), so every `nros_info!` call site puts
its own source path into rodata AND into the record body — and both leaves' paths
contain `er/s`.

**MEASURED, on the stale ELF:** the byte sequence `65 72 2f 73` occurs **zero
times in any ALLOCATED section** of either image (searched `.rodata_desc`,
`.rodata`, `.nros_boot_config`, `.data`, `.trap`, `.rwtext`, `.text`). It occurs
only in `.debug_line`, which is not loaded. If a search of the FAULTING build's
allocated sections also comes up empty, then the four bytes were assembled at
runtime (a keyexpr, a formatted log record) rather than copied out of rodata, and
the corruptor is a heap/network/log buffer rather than a static string.

### 3. MEASURED + source — `0x42051d70` cannot name the caller; it is a frame of the panic printer

`esp-hal`'s RISC-V `ExceptionHandler` does not print anything itself: it calls
`panic!("Exception '{}' mepc=… \n{:?}", code, mepc, mtval, context)`
(`esp-hal-1.0.0/src/exception_handler/mod.rs:112`). `esp-backtrace`'s
`panic_handler` then does, in this order (`esp-backtrace-0.18.1/src/lib.rs:105`):

1. `println!("{}", info)` — which formats the `TrapFrame` through
   `core::fmt::builders::DebugStruct`, and
2. only *then* `Backtrace::capture()`.

`capture()` reads its OWN frame pointer (`mv {0}, x8`) and walks the fp chain
(`riscv.rs:20`). That chain starts inside the panic handler and runs back
through `panic_fmt` → `ExceptionHandler` → the `_start_trap` stub. **It cannot
cross the trap boundary**: the interrupted function's frame pointer is a field
of the saved `TrapFrame`, not a link in the handler's chain. So no frame of the
faulting code can ever appear in that list, however many frames it prints.

Consistent with that, in the 2026-08-15 talker:

```
0x42051d70  ->  <core::fmt::builders::DebugStruct>::finish + 0x14
                (symbol starts at 0x42051d5c)
```

i.e. the address lands squarely in the formatting machinery that step (1) had
just run. That resolution is against the WRONG BUILD and is offered as
corroboration, not proof — but the structural argument above holds for any
build.

**"Where to start" step 1 should be struck.** It is not "the single
highest-value step"; it is a dead end by construction.

### 4. MEASURED (stale ELF) — the stack and the smoltcp buffers are neighbours, with no guard that works here

```
.bss            3fc896d0 .. 3fcbec20   (218448 B)
_stack_end   =  3fcbec20   == __ebss   — no gap, no guard page
_stack_start =  3fcce400
.stack size  =  0x0f7e0    =  63456 B  (~62 KB)

__stack_chk_guard = 3fcbec5c  (_stack_end + 0x3c)
```

What sits immediately BELOW the stack floor, in address order downward:

```
3fcbec1c  log::STATE, log::MAX_LOG_LEVEL_FILTER, esp_println::LOCK, …  (344 B of small statics)
3fcbe2c8  nros_smoltcp::TCP_TX_BUFFER_0   0x800
3fcbdac8  nros_smoltcp::TCP_RX_BUFFER_0   0x800
3fcbd2c8  nros_smoltcp::UDP_TX_DATA_1     0x800
3fcbcac8  nros_smoltcp::UDP_TX_DATA_0     0x800
3fcbc2c8  nros_smoltcp::UDP_RX_DATA_1     0x800
3fcbbac8  nros_smoltcp::UDP_RX_DATA_0     0x800
```

`TCP_TX_BUFFER_0` **ends 344 bytes below `_stack_end`**. A write past its end by
more than 344 bytes lands in the stack; a stack frame more than 344 bytes past
the floor lands in it. Nothing separates them.

The one guard that exists, `__stack_chk_guard`, is armed through the RISC-V
debug trigger CSRs (`tselect` / `tdata1` / `tdata2`, `esp-hal-1.0.0/src/debugger.rs`,
reached from `lib.rs:684` under `cfg(stack_guard_monitoring)`). The three guard
strings ARE in this image's rodata, so the feature is compiled in — but
**INFERRED**: QEMU's `esp32c3` machine does not implement the debug trigger
module, so those writes are inert and a stack overflow on QEMU presents as
silent corruption with no `Stack overflow detected` / `write to a stack guard
value` line. That is exactly what we see.

`nros-board-esp32-qemu/src/node.rs` already documents this failure mode
verbatim, from three previous rounds (#64 / #184 / #190):

> At 96 KB the stack shrank to ~18 KB … the overflow wrote frames straight down
> into `.bss` … producing the #190 phantom corruptions: InitAck cookies full of
> DRAM pointers …, **wild jumps (mepc=0x9ae65930)**, the pre-#190 0xffffffff
> config-pointer fault. None of them were allocator or zenoh-pico bugs.
>
> 48 KB fits the executor arena AND leaves a ~67 KB stack … **Check `.stack` in
> `readelf -S` after changing ANY large static** — there is no runtime
> stack-overflow guard on this target.

The comment claims ~67 KB. The measured `.stack` at the 2026-08-15 tree state is
**62 KB**, so ~5 KB of that headroom was already gone before the 1048 fix added
anything.

### 5. MEASURED — one more register, and the issue elides the evidence that would settle this

```
t2 = 1070320272 = 0x3fcbca90
```

In the stale layout that is **inside `nros_smoltcp::UDP_RX_DATA_1`**
(`3fcbc2c8 + 0x800 = 3fcbcac8`), at offset 0x7c8 — **56 bytes before the end of a
2 KB UDP receive buffer**. A live register pointing at the tail of an RX buffer,
in the same trap that shows a smashed return address, is a lead. (Addresses may
have moved in the faulting build; treat the containment as INFERRED there.)

The `TrapFrame` Debug output is quoted in this issue as
`{ ra: …, t0: 9, t1: 0, t2: …, … }` — **truncated**. The elided fields include
`sp` and `s0`. Those are the two that decide the whole question:

* `sp < 0x3fcbec20` (or whatever `_stack_end` is in the faulting build) is a
  **proof** of stack overflow, not a hypothesis.
* `s0` is the faulting function's frame pointer, from which the REAL backtrace
  can be walked by hand — the one the printed backtrace cannot reach (§3).

**This needs no rebuild.** Re-run the image that is already flashed and capture
the trap frame unabridged.

### 6. MEASURED — what actually differs between the two leaves, and why the bisect cut nearly all of it

At `origin/main`, `examples/qemu-esp32-baremetal/rust/{talker,listener}` differ
in: package / lib / bin name, `[package.metadata.nros.node] class` + `name`,
`Node::NAME`, `ENTITY_BOUNDS`, deploy `ip`, and the `register` / `on_callback`
bodies. Their `.cargo/config.toml` `[env]` blocks are **identical** (the files
differ by one space), so the arena / socket / payload knobs are not a variable.
The harness runs both through the same `start_esp32_qemu(…, networking=true)`
with byte-identical QEMU arguments.

Variants A–I plus the listener control have cut every one of those except the
crate name and `Node::NAME` (cut by I, with a caveat below). **That is itself a
result**: there is essentially no source-level difference left to blame, which
argues the trigger is not a difference between the leaves at all but a threshold
one of them crosses.

Two things the bisect should know:

* **`.nros_boot_config` in BOTH images has `set_flags = 0x0006`** — locator and
  domain only, `node_name` bit CLEAR, name bytes all zero. Both boot as
  `"nros_app"`. If variant I changed the Cargo.toml `[…node] name`, it changed
  nothing the board reads at boot, and the node name is only used inside
  `NodeOptions::new(…)` in `register` — which variant E had already emptied.
  (Stale-ELF measurement; re-check if the baking changed since 2026-08-15.)
* **The talker and listener DRAM maps were byte-identical** at that tree state:
  same `.data` size (0x8cd0), same `.bss` size (0x35550), same `.stack`
  (0x0f7e0), same symbol addresses throughout the `.bss` tail. `.text` differed
  by 1084 bytes, and `.text` is in flash. So "the talker's image is bigger, so
  its stack is smaller" is NOT the asymmetry — at least it was not then. Worth
  re-measuring on the current pair, because that is precisely what the 1048 fix
  could have changed.

### 7. "Memory pressure REFUTED" is over-claimed

Two problems with the `ZPICO_MAX_QUERYABLES` table:

1. **A constant faulting PC does not rule out stack corruption.** What a stack
   smash writes is DATA — here, four bytes of a string. String content is
   byte-identical in every build; only its ADDRESS moves. A deterministic
   overrun copying the same bytes over the same field of the same frame produces
   the same `mepc` every time. Constancy rules out *randomised* corruption, and
   nothing more.
2. **The three builds are only a layout experiment if that knob moves a static.**
   The table reports no `.bss` / `.stack` sizes. If `ZPICO_MAX_QUERYABLES` sizes
   something inside `g_sessions` (0x4948 = 18760 B of `.bss` in the stale
   talker) then the three had different `.stack` sizes and the experiment means
   something; if the queryable table is heap-side, all three had identical DRAM
   maps and the experiment measured nothing. `readelf -SW` on the three ELFs
   answers it in a second — and the board's own comment tells you to run exactly
   that after changing any large static.

### Strongest hypothesis, labelled

**INFERRED:** a stack frame's saved return address was overwritten with text,
because the stack and `.bss` interpenetrate on this board — the class this board
has already produced three times (#64, #184, #190), for which no runtime guard
can fire under QEMU. The bytes are ASCII because the thing sharing that memory
is a string buffer (a `nros_log` record carrying `core::file!()`, a keyexpr, or a
smoltcp buffer holding text).

**NOT a corrupted fn-pointer slot** — that is MEASURED-refuted by §1.

**Open, and honestly so:** why the talker and not the listener. §6 says the
source difference is nearly exhausted, so the answer is likely a threshold
(stack depth, or a buffer whose fill depends on what the router pushes back)
rather than a line of code.

### Cheapest experiments that would settle it, in order

1. **No build at all.** Re-run the already-flashed talker and capture the
   `TrapFrame` UNABRIDGED. `sp` below `_stack_end` proves stack overflow; `s0`
   gives the real backtrace. This is strictly more informative than every
   variant built so far.
2. **No build.** `riscv-none-elf-readelf -SW` on the current talker and listener
   ELFs and on the three `ZPICO_MAX_QUERYABLES` variants; record `.bss` end and
   `.stack` size for each. Settles §6's asymmetry and §7's item 2.
3. **No build.** Search the faulting build's ALLOCATED sections for `65 72 2f 73`:
   `riscv-none-elf-objcopy -O binary --only-section=.rodata <elf> /tmp/ro.bin`
   then grep. Present ⇒ the corruptor copies that string; absent (as it is in the
   stale build) ⇒ the bytes are built at runtime.
4. **One build, one variable.** Change ONLY the esp-alloc size in
   `node.rs` (`esp_alloc::heap_allocator!(size: 48 * 1024)` → `32 * 1024`).
   `.stack` grows by 16 KB and nothing else moves. If the fault vanishes or its
   `mepc` changes, it is stack depth. The board's comment already says the 48 KB
   is a two-sided constraint; no variant so far has touched that side.

### Corrections to earlier text in this issue

* The transcript under "What the image prints" shows `IP: 10.0.2.51/24`. The
  shipped talker's deploy `ip` is `10.0.2.50` (`.51` is the listener's), so that
  capture is from a MODIFIED talker — presumably variant G. Minor, but the
  opening evidence should say which image it is.
* `mepc`/`ra` read as `"s/rd"` / `"s/re"` is the big-endian reading; the load
  that restores `ra` is little-endian, so the bytes are `"er/s"` (§2).
* "a corrupted return address **or** function pointer" — the registers decide
  between them: return address (§1).
* "The same `Instruction access fault` appears twice in the tier-2 capture from
  BEFORE issue 1048's fix landed" rests on the exception NAME alone; no `mepc`
  from that capture is recorded here. Two instruction-access faults are not
  necessarily the same fault, and the pre-fix build had no `nros_log` record
  path in the leaves.

## RESOLVED, both halves: the frame is real and the bisect was stale (2026-09-04)

Ran the check the section above asked for, and it settles both.

An empty-`register` talker, **with the artifact verified this time** — the
`create_publisher` symbol count in the freshly built ELF is **0**, so the
publisher is genuinely gone. (`Hello World` is a BAD probe and stays present:
the string lives in the callback body, which still compiles. Symbol count is the
probe that discriminates.) Run against the same router, same QEMU arguments as
the harness (`-M esp32c3 -icount 3 -nic user,model=open_eth`), with the ORIGINAL
image kept as a matched control in the same session:

| image | `create_publisher` in ELF | faults in 60 s |
| --- | --- | --- |
| empty `register` | 0 | **0** |
| full talker (control) | present | **2** |

So `register` decides it, `ZenohSession::create_publisher` on the stack is real,
and **variants E, F and H are RETRACTED** — they measured the original image.

### Why they were stale: this issue's own sibling, #1025

`just esp32 build-qemu` resolved the ELF with

```
nros_fixture_row_artifact_dir "<leaf>" qemu-esp32-baremetal "" ""
```

— empty `cargo_args` and empty `envstr`, INVENTED at the call site rather than
read from the row. The talker row carries the variant `ZPICO_MAX_QUERYABLES=2`,
so cargo writes to the group dir `qemu-esp32-baremetal-4118800323` while the
packer looks in the bare `qemu-esp32-baremetal`. During E/F/H a stale ELF sat at
the bare path, so every "rebuild" packed the SAME original image, and three
variants agreed because they were one binary.

That is issue **#1025** exactly, and its fix (the by-id helper, which reads the
row instead of inventing its inputs) is in open PR **#303** — unmerged, which is
why the broken spelling is still on `main` and why the trap was still armed
today. It now fails LOUD (`is missing, and nothing narrowed this build`) rather
than packing a stale image, because #700 landed in between; that is the only
reason this was catchable at all.

### What is actually established

The fault is reached from `ZenohSession::create_publisher`, on the connected
path, in the talker only. The listener creates a subscription and does not
fault. Next step is that function, not another bisect.

### Method note

Every wrong turn in this issue has one shape: a measurement whose artifact
provenance was never checked — the ASCII lead, the "fault is on the CONNECTED
path" claim, and now E/F/H. The control that caught it costs one command
(`nm | grep -c <symbol>` on the ELF you are about to run) and would have caught
all three.
