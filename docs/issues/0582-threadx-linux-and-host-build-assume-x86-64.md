---
id: 582
title: "The host is assumed to be x86_64 in six places, and five of the six fail
  silently rather than loudly"
status: open
type: bug
area: build
related: [issue-0155, issue-0163, issue-0326, issue-0334, phase-338]
---

## Symptom

On an aarch64 Linux host, nothing about nano-ros's ThreadX-Linux support works,
and almost none of it says why. The failures look unrelated to each other and to
the host architecture.

A **seventh** instance of this class was found afterwards and is resolved
separately as [[0584]]: the board's log writer hardcoded the x86_64 `write`
syscall number, so off x86 every log line went to an unrelated syscall and the
image ran silently mute. Same shape, same invisibility on x86.

| where | what the user sees |
| --- | --- |
| `nros-c`, `nros-cpp`, zenoh service shim | `clippy::unnecessary_cast` under `-D warnings` — on ARM only |
| `nros_cdr_read_string` unit test | fails to compile: `[0i8; 32]` where the API takes `c_char` |
| `nros_threadx_setup_rust_lld` | **nothing** — an empty variable, then a link failure much later |
| `nros_threadx_strip_builtins` | **nothing** — same shape, same silence |
| every threadx-linux Rust leaf | cargo tries to CROSS COMPILE to x86_64 |
| ThreadX byte pools at run time | **nothing** — corrupted heap, no diagnostic |

These are one bug wearing six costumes: a place that means "the host" or "the
data model" was spelled `x86_64` instead.

## Mechanism

Three distinct spellings of the same mistake.

**1. `c_char` is not `i8` everywhere.** It is `i8` on x86 and `u8` on ARM and
aarch64. `ptr as *const u8` is therefore a real reinterpret on one and a no-op
on the other, and clippy lints the no-op — so the cast is *correct* on x86 and a
`-D warnings` build failure on ARM. The repo-wide idiom is `.cast::<u8>()`,
which compiles identically on both and is never linted. The same asymmetry makes
a hardcoded `[0i8; N]` buffer a type error against a `c_char` API off x86.

Note the shape of the pre-existing workaround in the zenoh service shim: an
`#[allow(clippy::unnecessary_cast)]` with a comment explaining the portability
concern. That silences the one site it is written on and teaches the next reader
to add a second one. It is the `#326` pattern — a second idiom where a shared
one belonged.

**2. The rustlib bin dir is under the HOST triple.** `rust-lld` and `llvm-ar`
live at `$(rustc --print sysroot)/lib/rustlib/<host>/bin`. Two `find_program`
lookups hardcoded `x86_64-unknown-linux-gnu` there, and — this is the part that
matters — both pass `NO_DEFAULT_PATH`. Off x86 the path simply does not exist,
`find_program` finds nothing, and the result is an **empty** `NROS_THREADX_*_PATH`
rather than an error. The build continues and fails somewhere else entirely.
`rustc -vV`'s `host:` line is the one authority for this and should be read once.

**3. A literal triple is not a host pin.** Six threadx-linux leaves and two
`fixtures.toml` rows carried `target = "x86_64-unknown-linux-gnu"` to express
"this is a host build" — ThreadX-Linux being a Linux-userspace simulation port
(pthreads-backed ThreadX, raw-socket NetX Duo). But a literal triple means "host
build" on exactly one machine and "cross compile" on every other. Omitting
`target` is what "host build" actually means; it also lands the artifact FLAT at
`target/<profile>/` with no triple segment, which is what the fixture resolvers
must then expect.

**4. Upstream ThreadX keyed its data model on the architecture.** Vendored
`ports/linux/gnu/inc/tx_port.h` branched on `__x86_64__` for two decisions whose
`#else` arms read as if "not x86_64" meant "32-bit host":

- LONG/ULONG are 32-bit by design even on 64-bit hosts
  (eclipse-threadx/threadx#532). Off x86_64 the narrowing was skipped, so ULONG
  became 8 bytes and every struct layout mirrored against the documented width
  disagreed with the kernel.
- `ALIGN_TYPE` defaults to ULONG when `ALIGN_TYPE_DEFINED` is absent, and byte
  pool blocks **store pointers in an ALIGN_TYPE**. Off x86_64 the default
  truncated an 8-byte pointer into 4. That one is heap corruption at run time
  with nothing to report it — the worst failure mode in the table above.

Keyed on `__LP64__` in the fork; ILP32 keeps the branch it already wanted.

## Adjacent defect found while fixing this

The ThreadX archives did not link on any host, for an unrelated reason that this
work surfaced: `libnros_platform_threadx.a` and the kernel archive were emitted
as plain static libs, but their only consumers (`platform_aliases.o`,
`c/platform/threadx/task.c`) arrive **bundled inside the zpico-sys rlib**, not
as their own `-l`. That puts the references after the archives on the link line,
so every member is discarded as unused before anything refers to it, and the
link dies on ~20 undefined `nros_platform_{tcp,udp}_*`. `+whole-archive` makes
resolution independent of position — the same force-link class as the RMW
backend's `#[no_mangle]` exports (issues 0155/0163).

Two traps here, both hit:

- The modifier must be set on the `cc::Build`, not as a second
  `cargo:rustc-link-lib` line. `compile()` already emits its own directive, and
  naming one lib twice with differing modifiers is a hard rustc error
  (`overriding linking modifiers from command line is not supported`).
- Whole-archiving the *kernel* is scoped to the hosted port deliberately. It
  pulls all ~190 objects, which is free for a Linux simulation binary and is
  exactly the image growth that matters on bare-metal RISC-V — and that port
  links fine as-is.

## Status

Fixed on `main`: the `.cast::<u8>()` sites, the `c_char` test buffer, all three
cmake lookups, the six leaves + two fixture rows, and the ThreadX fork
(`NEWSLabNTU/threadx` `nros-lp64-ulong`, committed as `b52acd8cf`).

Mechanism 2 is fixed as ONE helper, `nros_host_rustlib_bin()`, placed in the
cross-RTOS layer (`packages/api/nros-c/cmake/nros-rtos-helpers.cmake`) rather
than in `nros-threadx.cmake` — because the third caller is a toolchain file,
which cannot reach an RTOS-specific module. A first pass at this fix put the
helper in `nros-threadx.cmake` and left the toolchain file alone, which would
have been the #326 shape exactly: a helper plus a surviving second spelling.
Including the layer-1 module from a toolchain file is safe — it defines
functions behind an include sentinel and has no other top-level effect, so the
re-execution CMake does per `try_compile` costs two `rustc` invocations.

The RISC-V toolchain also no longer falls through when `rust-lld` is absent.
That silent `if(_RUST_LLD)` skip is *why* the hardcoded triple survived: off
x86 the lookup returned empty, the block was skipped, and the build degraded to
GNU ld — which then failed on the very picolibc TLS-`errno` mix the block exists
to avoid, naming neither rust-lld nor the toolchain file. It is now a
`FATAL_ERROR` naming the missing tool and the rustup component that ships it.

Verified on this aarch64 host: the helper resolves
`~/.rustup/toolchains/stable-aarch64-unknown-linux-gnu/lib/rustlib/aarch64-unknown-linux-gnu/bin`
and finds both `rust-lld` and `llvm-ar` there — where the hardcoded x86_64 path
found neither.

### Not yet fixed

**24 `system.toml` deploy blocks declare `target = "x86_64-unknown-linux-gnu"`.**
This looked like cosmetic drift and is not — it needs a real change, deliberately
deferred here rather than swept.

What was verified:

- It never becomes a cargo `--target`. `PLATFORM_TARGETS["native"]` is `None`,
  and the only non-test `--target` plumbing is the metadata probe, which already
  calls `host_triple()`.
- For `[deploy.native]` it is inert in the one predicate that reads it:
  `plan_target_is_hosted` (`planner.rs`) short-circuits on
  `matches!(build.board, "native" | "posix")` before reaching
  `build.target.contains("linux")`.
- **But it IS embedded in the resolved SystemModel.** Every
  `build/nros/models/<bringup>/*_model.yaml` carries `target:
  x86_64-unknown-linux-gnu`. The models are build artifacts, so on an aarch64
  host every workspace resolves a model asserting a triple that is not this
  machine's.

So deleting the 24 declarations is not the fix, and it is not free: it changes
model content for every workspace, which means re-resolution plus fixture
rebuilds to verify, and `board_descriptor.rs`'s `target_contains` matching reads
the same field for non-native deploys.

The fix is for `nros sync` to FILL `build.target` with `host_triple()` when a
`kind = "self"` deploy omits it — the declaration then stops being a hand-copied
constant and starts being a resolved fact, the same move #0582 made for the
rustlib dir. That is CLI behaviour plus a model-content change, i.e. phase work,
not a drive-by edit.

Note the six non-`native` sites need individual judgement and are not covered by
that rule: `[deploy.robot1]`/`[deploy.robot2]` in four workspaces declare a
REMOTE machine's triple (a real fact about another host, not this one), and
`[deploy.esp_idf_esp32c3]` / `[deploy.zephyr_native_sim]` in the test fixtures
declare x86_64 for an ESP32-C3 and a Zephyr sim respectively — the first of
those looks simply wrong and is worth a look on its own.

**Fixed: a stale comment.** `packages/boards/nros-board-threadx/build.rs` said
"threadx-linux is a real ThreadX board whose target IS
`x86_64-unknown-linux-gnu`" — true only on an x86 machine, and only because the
board pinned that literal until this issue removed it. The logic it justifies
was always arch-agnostic (it keys on the *port*); only the premise was wrong.

## Sweep

```sh
# mechanism 1 — c_char casts and i8 buffers
git grep -nE 'as \*(const|mut) u8' -- 'packages/**/*.rs' ':!third-party/'
git grep -n 'unnecessary_cast' -- ':!third-party/'
git grep -nE '\[0i8;|\[i8;' -- 'packages/**/*.rs' ':!third-party/'

# mechanisms 2 and 3 — hardcoded host triples
git grep -n 'x86_64-unknown-linux-gnu' -- ':!docs/' ':!book/' ':!*.md' ':!third-party/'

# mechanism 2 specifically — the silent-empty idiom
git grep -n -B2 -A2 'NO_DEFAULT_PATH' -- '*.cmake'
```

## Gate worth adding

Mechanism 2's signature is mechanical and greppable: a `find_program` with
`NO_DEFAULT_PATH` whose `PATHS` contains a literal target triple. A checker
rejecting that pattern would have caught all three sites at once, and would
catch the fourth. Mechanism 3's signature is a tracked `.cargo/config.toml`
whose `[build] target` equals the host triple — a pin that is always either a
no-op or a bug.

## Why a diagnosed defect survived a year

`docs/development/audit-findings-2026-07-28.md` line 77 (A1/A4, P2) already
recorded mechanism 2 — both sites, the `NO_DEFAULT_PATH` consequence, and the
resulting GNU-ld fallback into the picolibc `errno` failure — a year before this
issue. It was correct and specific, and nothing acted on it.

The plausible reason is that it was unreproducible for whoever read it: on an
x86 host every one of these six defects is invisible, so the finding reads as
theoretical. It became urgent only when someone built on aarch64. Audit findings
whose blast radius is "another host architecture" have no natural forcing
function, which argues for turning them into gates at audit time rather than
filing them as prose — see the checker above.
