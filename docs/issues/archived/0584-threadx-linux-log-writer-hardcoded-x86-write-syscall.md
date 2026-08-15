---
id: 584
title: "The ThreadX-Linux log writer hardcoded the x86_64 `write` syscall
  number, so off x86 the image was silently mute"
status: resolved
type: bug
area: platform-threadx
related: [issue-0582, issue-0155, issue-0163, issue-0243]
---

## Symptom

`logging_smoke_threadx_linux_captures_stderr` failed on the assertion, not on a
missing fixture:

```
Expected output to contain '[TRACE] smoke: trace payload', but it was not found.
```

The binary reported nothing wrong. Run directly it exited **0** and printed its
whole C-side boot sequence — byte pool, network init, app thread, `Calling Rust
entry...`, `Application completed successfully.` — while emitting none of its
six log lines. The kernel booted, the Rust entry ran and returned cleanly, and
the only missing thing was the output the fixture exists to produce.

Everything a reader would normally suspect was already ruled out by the fixture
itself: it sets `Severity::Trace` explicitly (not a threshold), calls
`nros_log::flush()` (not buffering), and the test spawns it bare (no unset env
knob).

## Root cause

`packages/boards/nros-board-threadx-linux/src/node.rs`:

```rust
const SYS_WRITE: isize = 1;
syscall(SYS_WRITE, STDERR_FD, line.as_ptr(), used);
```

**The Linux syscall number is per-ARCHITECTURE, not per-OS.** `write` is 1 on
x86_64 and **64** on every asm-generic port — aarch64, riscv64, loongarch64 —
where 1 is `io_destroy`. On aarch64 this writer therefore issued an unrelated
syscall, which failed, and the return value was discarded. No error, no partial
output, no diagnostic: a silently mute image.

The raw syscall is deliberate and had to stay. The ThreadX Linux port defines a
**weak `write`** that does not reach host fds, so a normal `write` call is
captured by it; going through `syscall` is what bypasses that. Only the number
was wrong.

This is a seventh instance of [[0582]]'s class — a place that means "this
machine" spelled as an x86 literal — and it shares that class's defining
property: on x86 it is invisible, and off x86 it fails without saying so.

## Fix

The write moved into the board's existing C glue
(`nros_board_log_write_stderr` in `c/board_threadx_linux.c`), which issues the
raw syscall with `SYS_write` from `<sys/syscall.h>`.

The first fix was a `cfg`-selected per-arch constant in Rust with a
`compile_error!` for unmapped architectures. That was correct but still a
hand-maintained table — a thing someone has to keep right for hosts nobody has
tried yet. The C headers already hold the answer for whatever host is
compiling, and they are the same source the `libc` crate's constants are
generated from, so asking them removes the table instead of making it more
accurate.

`libc::SYS_write` would also have been a genuine SSoT and was rejected for a
local reason: it needs a runtime `libc` dep on this board crate (today `libc`
is only a build-dependency here, via cbindgen → tempfile → getrandom), and
`examples/workspaces/{rust,realtime-rust}` carry
`[patch.crates-io] libc = { path = "third-party/nuttx/libc" }` documented as
NOT applying to the regular crate graph. Adding that edge would make the patch
start applying to the native/freertos/threadx/esp32 rows, silently swapping
their libc and invalidating a written invariant — a large blast radius for one
integer.

Verified on aarch64: the fixture emits all six severities in order, and both
`logging_smoke_threadx_linux_captures_stderr` and
`threadx_linux_entry_demos_build` pass.

Swept — this is the only raw `syscall()` caller and the only hardcoded syscall
number in `packages/`:

```sh
git grep -n 'fn syscall(' -- packages/ ':!third-party/'
git grep -nE 'SYS_[A-Z_]+\s*:\s*(isize|i64|usize|u64|c_long)\s*=' -- packages/ ':!third-party/'
```

## Two false trails, recorded so they are not re-walked

**Not attributable to #0582, and not bisectable.** Before #0582 this tree does
not link on aarch64 at all (the `nros_platform_{tcp,udp}_*` undefined-reference
set), so there was no baseline build to compare against. The defect long
predates that work; the fixture had simply never run on a non-x86 host.

**`+whole-archive` on `libglue.a` was the leading hypothesis and was wrong.**
`glue` holds the board's strong overrides of weak hooks, and losing them to
demand-driven member selection produces exactly this signature — runs, exits 0,
does nothing. The modifier was added and the symptom did not change. It was
kept on its own merits (three objects, and the reasoning stands), but it is not
what fixed this.

What actually settled it was `nm` on the linked image, per this issue's own
"next step": every symbol was present — `nros_platform_register_log_writer`,
`register_log_writer_public`, `PlatformSink::log` — which falsified the
link-class theory in one command and moved the search to what the writer *does*
rather than whether it *exists*.

## Lesson

The `nm` check cost one command and would have saved the whole `+whole-archive`
detour had it come first. When a program runs to completion and produces no
output, establish whether the code is *present* before theorising about why it
is *not reached* — absence and inaction look identical from the outside, and
only one of them is a link problem.
